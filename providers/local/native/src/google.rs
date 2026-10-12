//! Version-pinned native ABI from LiteRT-LM v0.18.0 C headers and MediaPipe 1.1.0 C bindings.
//! No interpreter or Python wrapper is loaded. Only the reviewed standalone shared libraries.
use libloading::Library;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    path::Path,
    ptr,
};
type P = *mut c_void;
fn c(value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| "invalid_arguments: embedded NUL".into())
}
fn path_c(path: &Path) -> Result<CString, String> {
    c(path
        .to_str()
        .ok_or("invalid_arguments: path must be UTF-8")?)
}
macro_rules! api {
    ($lib:expr,$name:literal,$ty:ty) => {{
        unsafe {
            *$lib
                .get::<$ty>(concat!($name, "\0").as_bytes())
                .map_err(|_| "unsupported_runtime: required native C symbol missing")?
        }
    }};
}
pub struct Gemma {
    lib: Library,
    engine: P,
}
impl Gemma {
    pub fn load(libpath: &Path, model: &Path, threads: i32) -> Result<Self, String> {
        let lib = unsafe { Library::new(libpath) }
            .map_err(|_| "unsupported_runtime: cannot load pinned LiteRT-LM library")?;
        let create = api!(
            lib,
            "litert_lm_embedding_engine_settings_create",
            unsafe extern "C" fn(*const c_char, *const c_char, *const c_char, *const c_char) -> P
        );
        let delete = api!(
            lib,
            "litert_lm_embedding_engine_settings_delete",
            unsafe extern "C" fn(P)
        );
        let setthreads = api!(
            lib,
            "litert_lm_embedding_engine_settings_set_num_threads",
            unsafe extern "C" fn(P, c_int)
        );
        let setmax = api!(
            lib,
            "litert_lm_embedding_engine_settings_set_max_input_length",
            unsafe extern "C" fn(P, c_int)
        );
        let make = api!(
            lib,
            "litert_lm_embedding_engine_create",
            unsafe extern "C" fn(P) -> P
        );
        let model = path_c(model)?;
        let cpu = c("cpu")?;
        let settings = unsafe { create(model.as_ptr(), cpu.as_ptr(), ptr::null(), ptr::null()) };
        if settings.is_null() {
            return Err("runtime_start_failed: native embedding settings failed".into());
        }
        unsafe {
            setthreads(settings, threads);
            setmax(settings, 2048);
        }
        let engine = unsafe { make(settings) };
        unsafe { delete(settings) };
        if engine.is_null() {
            return Err("runtime_start_failed: native embedding engine failed".into());
        }
        Ok(Self { lib, engine })
    }
    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let input = api!(
            self.lib,
            "litert_lm_input_data_create",
            unsafe extern "C" fn(c_int, *const c_void, usize) -> P
        );
        let freeinput = api!(
            self.lib,
            "litert_lm_input_data_delete",
            unsafe extern "C" fn(P)
        );
        let opts = api!(
            self.lib,
            "litert_lm_embedding_options_create",
            unsafe extern "C" fn() -> P
        );
        let freeopts = api!(
            self.lib,
            "litert_lm_embedding_options_delete",
            unsafe extern "C" fn(P)
        );
        let norm = api!(
            self.lib,
            "litert_lm_embedding_options_set_normalize",
            unsafe extern "C" fn(P, bool)
        );
        let special = api!(
            self.lib,
            "litert_lm_embedding_options_set_insert_special_tokens",
            unsafe extern "C" fn(P, bool)
        );
        let overflow = api!(
            self.lib,
            "litert_lm_embedding_options_set_input_overflow_strategy",
            unsafe extern "C" fn(P, c_int)
        );
        let compute = api!(
            self.lib,
            "litert_lm_embedding_engine_compute_embedding",
            unsafe extern "C" fn(P, *const P, usize, P) -> P
        );
        let len = api!(
            self.lib,
            "litert_lm_embedding_response_get_size",
            unsafe extern "C" fn(P) -> usize
        );
        let values = api!(
            self.lib,
            "litert_lm_embedding_response_get_values",
            unsafe extern "C" fn(P) -> *const f32
        );
        let freeres = api!(
            self.lib,
            "litert_lm_embedding_response_delete",
            unsafe extern "C" fn(P)
        );
        let data = unsafe { input(0, text.as_ptr().cast(), text.len()) };
        let options = unsafe { opts() };
        if data.is_null() || options.is_null() {
            unsafe {
                if !data.is_null() {
                    freeinput(data)
                }
                if !options.is_null() {
                    freeopts(options)
                }
            };
            return Err("model_inference_failed: native allocation failed".into());
        }
        unsafe {
            norm(options, true);
            special(options, true);
            overflow(options, 2);
        }
        let result = unsafe { compute(self.engine, &data, 1, options) };
        unsafe {
            freeinput(data);
            freeopts(options);
        }
        if result.is_null() {
            return Err("context_budget: native embedding rejected selected input".into());
        }
        let n = unsafe { len(result) };
        let p = unsafe { values(result) };
        let out = if n == 768 && !p.is_null() {
            Ok(unsafe { std::slice::from_raw_parts(p, n) }.to_vec())
        } else {
            Err("provider_schema_error: expected 768 dimensions".into())
        };
        unsafe { freeres(result) };
        out
    }
}
impl Drop for Gemma {
    fn drop(&mut self) {
        if let Ok(f) = unsafe {
            self.lib
                .get::<unsafe extern "C" fn(P)>(b"litert_lm_embedding_engine_delete\0")
        } {
            unsafe { f(self.engine) }
        }
    }
}
#[repr(C)]
struct Base {
    buffer: *const c_char,
    count: u32,
    path: *const c_char,
    fd: c_int,
    delegate: c_int,
    environment: c_int,
    system: c_int,
    version: *const c_char,
    ca: *const c_char,
    app: *const c_char,
    appver: *const c_char,
}
#[repr(C)]
struct Options {
    base: Base,
    max_tokens: c_int,
}
#[repr(C)]
struct BoolQ {
    condition: *const c_char,
    threshold: f32,
    temperature: f32,
    normalize_prior: bool,
}
#[derive(Debug, Default)]
#[repr(C)]
pub struct BoolR {
    pub value: bool,
    pub probability_true: f32,
    pub confidence: f32,
}
#[repr(C)]
struct ChoiceQ {
    keys: *const *const c_char,
    descs: *const *const c_char,
    count: c_int,
    temperature: f32,
    instructions: *const c_char,
    scoring_mode: c_int,
    normalize_prior: bool,
    images: *const *const u8,
    image_lengths: *const c_int,
    audios: *const *const f32,
    audio_lengths: *const c_int,
}
#[repr(C)]
struct ChoiceR {
    selected: *const c_char,
    keys: *const *const c_char,
    probabilities: *const f32,
    count: c_int,
    confidence: f32,
}
#[repr(C)]
struct ScoreQ {
    rubric: *const *const c_char,
    count: c_int,
    temperature: f32,
    instructions: *const c_char,
}
#[repr(C)]
struct ScoreR {
    selected: *const c_char,
    expected: f32,
    probabilities: *const f32,
    count: c_int,
    confidence: f32,
}
#[derive(Debug)]
pub struct ChoiceResult {
    pub value: String,
    pub probabilities: Vec<(String, f32)>,
    pub confidence: f32,
}
#[derive(Debug)]
pub struct ScoreResult {
    pub value: f32,
    pub level: String,
    pub probabilities: Vec<f32>,
    pub confidence: f32,
}
pub struct Laya {
    lib: Library,
    handle: P,
    prewarmed: std::collections::VecDeque<String>,
}
impl Laya {
    pub fn load(libpath: &Path, model: &Path) -> Result<Self, String> {
        let lib = unsafe { Library::new(libpath) }
            .map_err(|_| "unsupported_runtime: cannot load pinned MediaPipe library")?;
        let create = api!(
            lib,
            "MpDecisionMakerCreate",
            unsafe extern "C" fn(*const Options, *mut P, *mut *mut c_char) -> c_int
        );
        let path = path_c(model)?;
        let opts = Options {
            base: Base {
                buffer: ptr::null(),
                count: 0,
                path: path.as_ptr(),
                fd: -1,
                delegate: 0,
                environment: 0,
                system: 0,
                version: ptr::null(),
                ca: ptr::null(),
                app: ptr::null(),
                appver: ptr::null(),
            },
            max_tokens: 256,
        };
        let mut handle = ptr::null_mut();
        let mut err = ptr::null_mut();
        let status = unsafe { create(&opts, &mut handle, &mut err) };
        Self::status(&lib, status, err)?;
        if handle.is_null() {
            return Err("runtime_start_failed: native decision handle missing".into());
        }
        Ok(Self {
            lib,
            handle,
            prewarmed: std::collections::VecDeque::new(),
        })
    }
    fn status(lib: &Library, status: c_int, err: *mut c_char) -> Result<(), String> {
        if !err.is_null() {
            let free = api!(lib, "MpErrorFree", unsafe extern "C" fn(*mut c_char));
            unsafe { free(err) }
        }
        if status == 0 {
            Ok(())
        } else {
            Err(format!(
                "model_inference_failed: native decision failed (status {status}); diagnostics suppressed"
            ))
        }
    }
    pub fn boolean(
        &mut self,
        state: &str,
        condition: &str,
        threshold: f32,
    ) -> Result<BoolR, String> {
        let f = api!(
            self.lib,
            "MpDecisionMakerEvaluateBoolean",
            unsafe extern "C" fn(
                P,
                *const c_char,
                *const BoolQ,
                *mut BoolR,
                *mut *mut c_char,
            ) -> c_int
        );
        let s = c(state)?;
        let q = c(condition)?;
        let question = BoolQ {
            condition: q.as_ptr(),
            threshold,
            temperature: 0.0,
            normalize_prior: false,
        };
        let mut result = BoolR::default();
        let mut err = ptr::null_mut();
        let status = unsafe { f(self.handle, s.as_ptr(), &question, &mut result, &mut err) };
        Self::status(&self.lib, status, err)?;
        Ok(result)
    }
    pub fn choice(
        &mut self,
        state: &str,
        keys: &[String],
        descriptions: &[String],
        instructions: &str,
    ) -> Result<ChoiceResult, String> {
        if keys.len() != descriptions.len() || !(2..=8).contains(&keys.len()) {
            return Err("invalid_arguments: choice requires 2–8 options".into());
        }
        let f = api!(
            self.lib,
            "MpDecisionMakerEvaluateChoice",
            unsafe extern "C" fn(
                P,
                *const c_char,
                *const ChoiceQ,
                *mut ChoiceR,
                *mut *mut c_char,
            ) -> c_int
        );
        let close = api!(
            self.lib,
            "MpDecisionMakerCloseChoiceResult",
            unsafe extern "C" fn(*mut ChoiceR)
        );
        let s = c(state)?;
        let instr = c(instructions)?;
        let keys = keys.iter().map(|v| c(v)).collect::<Result<Vec<_>, _>>()?;
        let descs = descriptions
            .iter()
            .map(|v| c(v))
            .collect::<Result<Vec<_>, _>>()?;
        let kp = keys.iter().map(|v| v.as_ptr()).collect::<Vec<_>>();
        let dp = descs.iter().map(|v| v.as_ptr()).collect::<Vec<_>>();
        let q = ChoiceQ {
            keys: kp.as_ptr(),
            descs: dp.as_ptr(),
            count: kp.len() as i32,
            temperature: 0.0,
            instructions: instr.as_ptr(),
            scoring_mode: 0,
            normalize_prior: false,
            images: ptr::null(),
            image_lengths: ptr::null(),
            audios: ptr::null(),
            audio_lengths: ptr::null(),
        };
        let mut r = ChoiceR {
            selected: ptr::null(),
            keys: ptr::null(),
            probabilities: ptr::null(),
            count: 0,
            confidence: 0.0,
        };
        let mut err = ptr::null_mut();
        let schema = format!("{keys:?}{descs:?}{instr:?}");
        if !self.prewarmed.contains(&schema) {
            let prewarm = api!(
                self.lib,
                "MpDecisionMakerPrewarmChoice",
                unsafe extern "C" fn(P, *const ChoiceQ, *mut *mut c_char) -> c_int
            );
            let status = unsafe { prewarm(self.handle, &q, &mut err) };
            Self::status(&self.lib, status, err)?;
            err = ptr::null_mut();
            self.prewarmed.push_back(schema);
            if self.prewarmed.len() > 16 {
                self.prewarmed.pop_front();
            }
        }
        let status = unsafe { f(self.handle, s.as_ptr(), &q, &mut r, &mut err) };
        let out = (|| {
            Self::status(&self.lib, status, err)?;
            if r.count != kp.len() as i32
                || r.keys.is_null()
                || r.probabilities.is_null()
                || r.selected.is_null()
            {
                return Err("provider_schema_error: native choice count/pointers invalid".into());
            }
            let mut probabilities = Vec::new();
            for i in 0..kp.len() {
                let key = unsafe { *r.keys.add(i) };
                if key.is_null() {
                    return Err("provider_schema_error: missing key".into());
                }
                probabilities.push((
                    unsafe { CStr::from_ptr(key) }
                        .to_str()
                        .map_err(|_| "provider_schema_error: key UTF-8")?
                        .to_owned(),
                    unsafe { *r.probabilities.add(i) },
                ));
            }
            Ok(ChoiceResult {
                value: unsafe { CStr::from_ptr(r.selected) }
                    .to_str()
                    .map_err(|_| "provider_schema_error: key UTF-8")?
                    .into(),
                probabilities,
                confidence: r.confidence,
            })
        })();
        unsafe { close(&mut r) };
        out
    }
    pub fn score(
        &mut self,
        state: &str,
        rubric: &[String],
        instructions: &str,
    ) -> Result<ScoreResult, String> {
        let f = api!(
            self.lib,
            "MpDecisionMakerEvaluateScore",
            unsafe extern "C" fn(
                P,
                *const c_char,
                *const ScoreQ,
                *mut ScoreR,
                *mut *mut c_char,
            ) -> c_int
        );
        let close = api!(
            self.lib,
            "MpDecisionMakerCloseScoreResult",
            unsafe extern "C" fn(*mut ScoreR)
        );
        let s = c(state)?;
        let instr = c(instructions)?;
        let rub = rubric.iter().map(|v| c(v)).collect::<Result<Vec<_>, _>>()?;
        let rp = rub.iter().map(|v| v.as_ptr()).collect::<Vec<_>>();
        let q = ScoreQ {
            rubric: rp.as_ptr(),
            count: rp.len() as i32,
            temperature: 0.0,
            instructions: instr.as_ptr(),
        };
        let mut r = ScoreR {
            selected: ptr::null(),
            expected: 0.0,
            probabilities: ptr::null(),
            count: 0,
            confidence: 0.0,
        };
        let mut err = ptr::null_mut();
        let status = unsafe { f(self.handle, s.as_ptr(), &q, &mut r, &mut err) };
        let out = (|| {
            Self::status(&self.lib, status, err)?;
            if r.count != rp.len() as i32 || r.probabilities.is_null() || r.selected.is_null() {
                return Err("provider_schema_error: native score count/pointers invalid".into());
            }
            Ok(ScoreResult {
                value: r.expected,
                level: unsafe { CStr::from_ptr(r.selected) }
                    .to_str()
                    .map_err(|_| "provider_schema_error: level UTF-8")?
                    .into(),
                probabilities: unsafe { std::slice::from_raw_parts(r.probabilities, rp.len()) }
                    .to_vec(),
                confidence: r.confidence,
            })
        })();
        unsafe { close(&mut r) };
        out
    }
}
impl Drop for Laya {
    fn drop(&mut self) {
        if let (Ok(close), Ok(free)) = (
            unsafe {
                self.lib
                    .get::<unsafe extern "C" fn(P, *mut *mut c_char) -> c_int>(
                        b"MpDecisionMakerClose\0",
                    )
            },
            unsafe {
                self.lib
                    .get::<unsafe extern "C" fn(*mut c_char)>(b"MpErrorFree\0")
            },
        ) {
            let mut err = ptr::null_mut();
            unsafe {
                close(self.handle, &mut err);
                if !err.is_null() {
                    free(err)
                }
            }
        }
    }
}
// The released C ABI is 64-bit on every reviewed vendor platform. A wheel upgrade requires review.
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(std::mem::size_of::<Base>() == 72);
    assert!(std::mem::size_of::<Options>() == 80);
    assert!(std::mem::offset_of!(Base, path) == 16);
    assert!(std::mem::offset_of!(Base, version) == 40);
    assert!(std::mem::size_of::<BoolQ>() == 24);
    assert!(std::mem::offset_of!(BoolQ, normalize_prior) == 16);
    assert!(std::mem::size_of::<BoolR>() == 12);
    assert!(std::mem::size_of::<ChoiceQ>() == 72);
    assert!(std::mem::offset_of!(ChoiceQ, images) == 40);
    assert!(std::mem::size_of::<ChoiceR>() == 32);
    assert!(std::mem::size_of::<ScoreQ>() == 24);
    assert!(std::mem::size_of::<ScoreR>() == 32);
};
