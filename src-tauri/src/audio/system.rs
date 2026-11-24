//! Audio del sistema (lo que suena por los altavoces: Meet, Zoom, Teams…) con los
//! *process taps* de Core Audio (macOS 14.2+), como Wispr Flow. Solo necesita el
//! permiso «Solo grabación del audio del sistema», no el de Grabación de pantalla.
//!
//! Montaje: un tap global en mono que excluye a Susurro (para no grabar sus propios
//! sonidos) → un dispositivo agregado privado que lo lee → un IOProc que copia las
//! muestras a un búfer. Se guarda a la frecuencia del tap y se pasa a 16 kHz al vaciar.

use std::ffi::{CStr, c_void};
use std::ptr::NonNull;
use std::sync::Arc;

use anyhow::{Result, bail};
use block2::RcBlock;
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_audio::{
    AudioDeviceCreateIOProcIDWithBlock, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart,
    AudioDeviceStop, AudioHardwareCreateAggregateDevice, AudioHardwareCreateProcessTap,
    AudioHardwareDestroyAggregateDevice, AudioHardwareDestroyProcessTap, AudioObjectGetPropertyData, AudioObjectID,
    AudioObjectPropertyAddress, AudioObjectPropertySelector, CATapDescription, CATapMuteBehavior,
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey, kAudioAggregateDeviceTapAutoStartKey,
    kAudioAggregateDeviceTapListKey, kAudioAggregateDeviceUIDKey, kAudioHardwarePropertyTranslatePIDToProcessObject,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
    kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey, kAudioTapPropertyFormat,
};
use objc2_core_audio_types::{AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSUUID};
use parking_lot::Mutex;
use susurro_core::audio::{WHISPER_RATE, resample, to_mono};

type IoBlock = RcBlock<
    dyn Fn(
        NonNull<AudioTimeStamp>,
        NonNull<AudioBufferList>,
        NonNull<AudioTimeStamp>,
        NonNull<AudioBufferList>,
        NonNull<AudioTimeStamp>,
    ),
>;

pub struct SystemCapture {
    tap: AudioObjectID,
    device: AudioObjectID,
    proc_id: AudioDeviceIOProcID,
    _block: Option<IoBlock>,
    running: bool,
    rate: u32,
    buf: Arc<Mutex<Vec<f32>>>,
}

// Los identificadores de Core Audio son números y el bloque solo lo toca el hilo de
// audio; la reunión mueve la captura a su propio hilo.
unsafe impl Send for SystemCapture {}

impl SystemCapture {
    pub fn start() -> Result<Self> {
        // Se va rellenando paso a paso; si algo falla, `Drop` deshace lo ya creado.
        let mut cap = Self {
            tap: 0,
            device: 0,
            proc_id: None,
            _block: None,
            running: false,
            rate: WHISPER_RATE,
            buf: Arc::new(Mutex::new(Vec::new())),
        };

        // 1. Tap global en mono, sin el audio de Susurro.
        let own: Vec<Retained<NSNumber>> = own_process_object().map(NSNumber::new_u32).into_iter().collect();
        let exclude = NSArray::from_retained_slice(&own);
        let desc = unsafe {
            let d = CATapDescription::initMonoGlobalTapButExcludeProcesses(CATapDescription::alloc(), &exclude);
            d.setPrivate(true);
            d.setMuteBehavior(CATapMuteBehavior::Unmuted);
            d
        };
        let status = unsafe { AudioHardwareCreateProcessTap(Some(&desc), &mut cap.tap) };
        if status != 0 || cap.tap == 0 {
            bail!("sin acceso al audio del sistema (¿permiso de Audio del sistema?), código {status}");
        }

        // 2. Formato real del tap (suele ser 48 kHz, f32).
        let format: AudioStreamBasicDescription = get_property(cap.tap, kAudioTapPropertyFormat)
            .ok_or_else(|| anyhow::anyhow!("no se pudo leer el formato del audio del sistema"))?;
        cap.rate = format.mSampleRate.round() as u32;
        let channels = format.mChannelsPerFrame.max(1) as usize;

        // 3. Dispositivo agregado privado que lee el tap.
        let tap_uid = unsafe { desc.UUID().UUIDString() };
        let device_uid = NSUUID::new().UUIDString();
        let sub_tap = dict(&[
            (kAudioSubTapUIDKey, &*tap_uid),
            (kAudioSubTapDriftCompensationKey, &*NSNumber::new_bool(true)),
        ]);
        let taps = NSArray::from_retained_slice(&[sub_tap]);
        let agg = dict(&[
            (kAudioAggregateDeviceUIDKey, &*device_uid),
            (kAudioAggregateDeviceNameKey, &*NSString::from_str("Susurro audio del sistema")),
            (kAudioAggregateDeviceIsPrivateKey, &*NSNumber::new_bool(true)),
            (kAudioAggregateDeviceTapAutoStartKey, &*NSNumber::new_bool(true)),
            (kAudioAggregateDeviceTapListKey, &*taps),
        ]);
        // NSDictionary y CFDictionary son intercambiables (toll-free bridging).
        let cf: &CFDictionary = unsafe { &*(Retained::as_ptr(&agg) as *const CFDictionary) };
        let status = unsafe { AudioHardwareCreateAggregateDevice(cf, NonNull::from(&mut cap.device)) };
        if status != 0 || cap.device == 0 {
            bail!("no se pudo preparar la captura del audio del sistema, código {status}");
        }

        // 4. IOProc: copia lo que llega a nuestro búfer.
        let sink = cap.buf.clone();
        let block: IoBlock = RcBlock::new(
            move |_now: NonNull<AudioTimeStamp>,
                  input: NonNull<AudioBufferList>,
                  _in_time: NonNull<AudioTimeStamp>,
                  _output: NonNull<AudioBufferList>,
                  _out_time: NonNull<AudioTimeStamp>| {
                let list = unsafe { input.as_ref() };
                if list.mNumberBuffers == 0 {
                    return;
                }
                let b = &list.mBuffers[0];
                if b.mData.is_null() || b.mDataByteSize == 0 {
                    return;
                }
                let samples =
                    unsafe { std::slice::from_raw_parts(b.mData as *const f32, b.mDataByteSize as usize / 4) };
                let ch = (b.mNumberChannels as usize).max(1).min(channels.max(1));
                if ch > 1 {
                    sink.lock().extend(to_mono(samples, ch));
                } else {
                    sink.lock().extend_from_slice(samples);
                }
            },
        );
        let status = unsafe {
            AudioDeviceCreateIOProcIDWithBlock(
                NonNull::from(&mut cap.proc_id),
                cap.device,
                None,
                &*block as *const _ as *mut _,
            )
        };
        cap._block = Some(block);
        if status != 0 || cap.proc_id.is_none() {
            bail!("no se pudo leer el audio del sistema, código {status}");
        }

        let status = unsafe { AudioDeviceStart(cap.device, cap.proc_id) };
        if status != 0 {
            bail!("no se pudo empezar a capturar el audio del sistema, código {status}");
        }
        cap.running = true;
        Ok(cap)
    }

    /// Todo lo capturado desde la última llamada, mono a 16 kHz.
    pub fn drain(&self) -> Vec<f32> {
        let raw = std::mem::take(&mut *self.buf.lock());
        if self.rate == WHISPER_RATE { raw } else { resample(&raw, self.rate, WHISPER_RATE) }
    }

    pub fn stop(mut self) -> Vec<f32> {
        self.teardown();
        self.drain()
    }

    fn teardown(&mut self) {
        unsafe {
            if self.running {
                AudioDeviceStop(self.device, self.proc_id);
                self.running = false;
            }
            if self.proc_id.is_some() {
                AudioDeviceDestroyIOProcID(self.device, self.proc_id);
                self.proc_id = None;
            }
            if self.device != 0 {
                AudioHardwareDestroyAggregateDevice(self.device);
                self.device = 0;
            }
            if self.tap != 0 {
                AudioHardwareDestroyProcessTap(self.tap);
                self.tap = 0;
            }
        }
        self._block = None;
    }
}

impl Drop for SystemCapture {
    fn drop(&mut self) {
        self.teardown();
    }
}

/// El objeto de proceso de Core Audio de Susurro, para excluirlo del tap.
fn own_process_object() -> Option<u32> {
    let pid = std::process::id() as i32;
    let addr = address(kAudioHardwarePropertyTranslatePIDToProcessObject);
    let mut obj: AudioObjectID = 0;
    let mut size = std::mem::size_of::<AudioObjectID>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject as AudioObjectID,
            NonNull::from(&addr),
            std::mem::size_of::<i32>() as u32,
            &pid as *const i32 as *const c_void,
            NonNull::from(&mut size),
            NonNull::from(&mut obj).cast(),
        )
    };
    (status == 0 && obj != 0).then_some(obj)
}

fn address(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// Lee una propiedad de tamaño fijo (struct C sin punteros, así que vale empezar a ceros).
fn get_property<T: Copy>(object: AudioObjectID, selector: AudioObjectPropertySelector) -> Option<T> {
    let addr = address(selector);
    let mut value: T = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<T>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    (status == 0).then_some(value)
}

fn dict(pairs: &[(&CStr, &AnyObject)]) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<Retained<NSString>> =
        pairs.iter().map(|(k, _)| NSString::from_str(k.to_str().unwrap_or_default())).collect();
    let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
    let values: Vec<&AnyObject> = pairs.iter().map(|(_, v)| *v).collect();
    NSDictionary::from_slices(&key_refs, &values)
}
