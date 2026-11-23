//! macOS backend (EXPERIMENTAL): the private `kperf` / `kperfdata` frameworks.
//!
//! **Status.** This was written without access to a Mac and has not been run
//! against real counters. It is built from how Apple's own tooling and open-source
//! `kpc` demos use these frameworks. Every failure, from a missing framework to a
//! rejected call, becomes [`Unavailable`] with a reason, and nothing here panics.
//! See `docs/hardware-counters.md`.
//!
//! **Why it is risky.** `kperf` is a private API with no header and no stability
//! promise. The frameworks are found by name at run time and each function by
//! `dlsym`. A missing symbol is detected, but a *changed signature* is not, so a
//! future macOS release could make a call misbehave. It needs root, because the
//! counters are system-wide state, and it takes them from other users of the PMU
//! (Instruments, `sudo`'d profilers) while a `Kpc` is alive.
//!
//! **How it works.** `kpep_*` (the event database) turns an event name into a
//! counter configuration for this CPU, `kpc_set_config` and `kpc_set_counting`
//! program and start it, and `kpc_get_thread_counters` reads the calling
//! thread's running counts, which `start` and `end` subtract.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::mem::size_of_val;

use super::{Counter, Unavailable, kpep_event_names, layer_name, pick_first_available};

const KPERF: &CStr = c"/System/Library/PrivateFrameworks/kperf.framework/kperf";
const KPERFDATA: &CStr = c"/System/Library/PrivateFrameworks/kperfdata.framework/kperfdata";

/// Room for the largest counter set the kernel hands out (`KPC_MAX_COUNTERS`).
const MAX_COUNTERS: usize = 32;

type SetInt = unsafe extern "C" fn(c_int) -> c_int;
type SetClasses = unsafe extern "C" fn(u32) -> c_int;
type SetConfig = unsafe extern "C" fn(u32, *mut u64) -> c_int;
type GetThreadCounters = unsafe extern "C" fn(u32, u32, *mut u64) -> c_int;
type DbCreate = unsafe extern "C" fn(*const c_char, *mut *mut c_void) -> c_int;
type Free = unsafe extern "C" fn(*mut c_void);
type DbEvent = unsafe extern "C" fn(*mut c_void, *const c_char, *mut *mut c_void) -> c_int;
type ConfigCreate = unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> c_int;
type ConfigOnly = unsafe extern "C" fn(*mut c_void) -> c_int;
type AddEvent = unsafe extern "C" fn(*mut c_void, *mut *mut c_void, u32, *mut u32) -> c_int;
type ConfigKpc = unsafe extern "C" fn(*mut c_void, *mut u64, usize) -> c_int;
type ConfigCount = unsafe extern "C" fn(*mut c_void, *mut usize) -> c_int;
type ConfigClasses = unsafe extern "C" fn(*mut c_void, *mut u32) -> c_int;
type ConfigMap = unsafe extern "C" fn(*mut c_void, *mut usize, usize) -> c_int;

/// The functions used, resolved once. The signatures are from memory of how the
/// frameworks are used elsewhere and cannot be checked against a header.
struct Api {
    force_all_ctrs_set: SetInt,
    set_counting: SetClasses,
    set_thread_counting: SetClasses,
    set_config: SetConfig,
    get_thread_counters: GetThreadCounters,
    db_create: DbCreate,
    db_free: Free,
    db_event: DbEvent,
    config_create: ConfigCreate,
    config_free: Free,
    config_force_counters: ConfigOnly,
    config_add_event: AddEvent,
    config_kpc: ConfigKpc,
    config_kpc_count: ConfigCount,
    config_kpc_classes: ConfigClasses,
    config_kpc_map: ConfigMap,
}

fn unavailable(counter: Counter, why: impl std::fmt::Display) -> Unavailable {
    Unavailable {
        reason: format!(
            "{}: {why}. The macOS backend is experimental and uses a private API; \
             see docs/hardware-counters.md",
            layer_name(counter)
        ),
    }
}

/// Resolves `name` in `lib`, as the function-pointer type `T`.
///
/// # Safety
///
/// `T` must be a function-pointer type, and `lib` a handle from `dlopen`. The
/// caller is trusting that the symbol really has the signature `T` says.
unsafe fn symbol<T: Copy>(lib: *mut c_void, name: &CStr) -> Option<T> {
    debug_assert_eq!(size_of::<T>(), size_of::<*mut c_void>());
    // SAFETY: `lib` is a live handle and `name` is NUL-terminated.
    let ptr = unsafe { libc::dlsym(lib, name.as_ptr()) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: `T` is pointer-sized (checked above in debug builds) and the caller
    // vouches for the signature. A null pointer was ruled out above.
    Some(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&ptr) })
}

impl Api {
    fn load(counter: Counter) -> Result<Self, Unavailable> {
        // SAFETY: both paths are NUL-terminated string literals. `dlopen` has no
        // other preconditions; it returns null on failure.
        let (kperf, kperfdata) = unsafe {
            (
                libc::dlopen(KPERF.as_ptr(), libc::RTLD_LAZY),
                libc::dlopen(KPERFDATA.as_ptr(), libc::RTLD_LAZY),
            )
        };
        if kperf.is_null() || kperfdata.is_null() {
            return Err(unavailable(
                counter,
                "the kperf frameworks could not be loaded",
            ));
        }
        macro_rules! get {
            ($lib:expr, $name:literal) => {
                // SAFETY: `$lib` is a live handle. The signature is the one this
                // module declares for the function, which is the documented risk.
                match unsafe { symbol($lib, $name) } {
                    Some(f) => f,
                    None => {
                        return Err(unavailable(
                            counter,
                            format!("symbol {} is missing", $name.to_string_lossy()),
                        ));
                    }
                }
            };
        }
        Ok(Self {
            force_all_ctrs_set: get!(kperf, c"kpc_force_all_ctrs_set"),
            set_counting: get!(kperf, c"kpc_set_counting"),
            set_thread_counting: get!(kperf, c"kpc_set_thread_counting"),
            set_config: get!(kperf, c"kpc_set_config"),
            get_thread_counters: get!(kperf, c"kpc_get_thread_counters"),
            db_create: get!(kperfdata, c"kpep_db_create"),
            db_free: get!(kperfdata, c"kpep_db_free"),
            db_event: get!(kperfdata, c"kpep_db_event"),
            config_create: get!(kperfdata, c"kpep_config_create"),
            config_free: get!(kperfdata, c"kpep_config_free"),
            config_force_counters: get!(kperfdata, c"kpep_config_force_counters"),
            config_add_event: get!(kperfdata, c"kpep_config_add_event"),
            config_kpc: get!(kperfdata, c"kpep_config_kpc"),
            config_kpc_count: get!(kperfdata, c"kpep_config_kpc_count"),
            config_kpc_classes: get!(kperfdata, c"kpep_config_kpc_classes"),
            config_kpc_map: get!(kperfdata, c"kpep_config_kpc_map"),
        })
    }
}

/// A running counter for the calling thread. The PMU is system-wide state, so
/// only one of these should exist at a time; dropping it switches counting off.
pub(crate) struct Kpc {
    api: Api,
    /// Index of our event in the per-thread counter array.
    index: usize,
    start: u64,
}

impl std::fmt::Debug for Kpc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kpc").field("index", &self.index).finish()
    }
}

impl Kpc {
    pub(crate) fn open(counter: Counter) -> Result<Self, Unavailable> {
        let api = Api::load(counter)?;

        // SAFETY: takes a plain int. Returns non-zero without root, which is
        // reported below.
        if unsafe { (api.force_all_ctrs_set)(1) } != 0 {
            return Err(unavailable(
                counter,
                "taking the counters failed; this needs root, so run the benchmark with sudo",
            ));
        }
        match Self::configure(&api, counter) {
            Ok(index) => Ok(Self {
                api,
                index,
                start: 0,
            }),
            Err(err) => {
                // SAFETY: undoes the call above.
                unsafe { (api.force_all_ctrs_set)(0) };
                Err(err)
            }
        }
    }

    /// Builds the configuration for `counter`, starts counting, and returns the
    /// event's index in the counter array.
    fn configure(api: &Api, counter: Counter) -> Result<usize, Unavailable> {
        let mut db: *mut c_void = std::ptr::null_mut();
        // SAFETY: a null name selects the current CPU's database. `db` is a live
        // out-pointer.
        if unsafe { (api.db_create)(std::ptr::null(), &mut db) } != 0 || db.is_null() {
            return Err(unavailable(counter, "no event database for this CPU"));
        }
        let result = Self::configure_with_db(api, counter, db);
        // SAFETY: `db` came from `db_create` and is freed once here. The
        // configuration made from it has already been consumed.
        unsafe { (api.db_free)(db) };
        result
    }

    fn configure_with_db(
        api: &Api,
        counter: Counter,
        db: *mut c_void,
    ) -> Result<usize, Unavailable> {
        let mut event: *mut c_void = std::ptr::null_mut();
        let found = pick_first_available(kpep_event_names(counter), |name| {
            let Ok(name) = CString::new(name) else {
                return false;
            };
            event = std::ptr::null_mut();
            // SAFETY: `db` is live, `name` is NUL-terminated, `event` is a live
            // out-pointer.
            unsafe { (api.db_event)(db, name.as_ptr(), &mut event) == 0 && !event.is_null() }
        });
        let Some(event_name) = found else {
            return Err(unavailable(
                counter,
                "none of the candidate events exists on this CPU",
            ));
        };

        let mut config: *mut c_void = std::ptr::null_mut();
        // SAFETY: `db` is live and `config` is a live out-pointer.
        if unsafe { (api.config_create)(db, &mut config) } != 0 || config.is_null() {
            return Err(unavailable(counter, "could not create a configuration"));
        }
        let result = Self::program(api, config, &mut event);
        // SAFETY: `config` came from `config_create` and is freed once here.
        unsafe { (api.config_free)(config) };
        result.map_err(|why| unavailable(counter, format!("{why} (event {event_name})")))
    }

    fn program(api: &Api, config: *mut c_void, event: &mut *mut c_void) -> Result<usize, String> {
        let mut err = 0_u32;
        // SAFETY: `config` is live, `event` points at an event from the same
        // database, `err` is a live out-pointer.
        unsafe {
            if (api.config_force_counters)(config) != 0 {
                return Err("could not claim the counters".to_owned());
            }
            if (api.config_add_event)(config, event, 0, &mut err) != 0 {
                return Err(format!("the event was rejected (code {err})"));
            }
        }

        let mut classes = 0_u32;
        let mut count = 0_usize;
        let mut regs = [0_u64; MAX_COUNTERS];
        let mut map = [0_usize; MAX_COUNTERS];
        // SAFETY: every pointer is to a live local, and each size passed is the
        // size in bytes of the buffer it describes.
        let rc = unsafe {
            (api.config_kpc_classes)(config, &mut classes)
                | (api.config_kpc_count)(config, &mut count)
                | (api.config_kpc)(config, regs.as_mut_ptr(), size_of_val(&regs))
                | (api.config_kpc_map)(config, map.as_mut_ptr(), size_of_val(&map))
        };
        if rc != 0 || count == 0 || count > MAX_COUNTERS {
            return Err("the configuration could not be read back".to_owned());
        }
        // One event was added, so its slot is the first entry of the map.
        let index = map[0];
        if index >= MAX_COUNTERS {
            return Err("the kernel returned an out-of-range counter index".to_owned());
        }

        // SAFETY: `regs` holds the configuration that `config_kpc` just wrote for
        // `classes`.
        let rc = unsafe {
            let a = (api.set_config)(classes, regs.as_mut_ptr());
            let b = (api.set_counting)(classes);
            let c = (api.set_thread_counting)(classes);
            a | b | c
        };
        if rc != 0 {
            // SAFETY: switches counting off again.
            unsafe {
                (api.set_counting)(0);
                (api.set_thread_counting)(0);
            }
            return Err("starting the counters failed".to_owned());
        }
        Ok(index)
    }

    fn read_now(&self) -> Option<u64> {
        let mut buf = [0_u64; MAX_COUNTERS];
        // SAFETY: thread id 0 means the calling thread, and `buf` holds exactly
        // `MAX_COUNTERS` writable `u64`s.
        let rc =
            unsafe { (self.api.get_thread_counters)(0, MAX_COUNTERS as u32, buf.as_mut_ptr()) };
        (rc == 0).then(|| buf[self.index])
    }

    pub(crate) fn start(&mut self) {
        self.start = self.read_now().unwrap_or(0);
    }

    pub(crate) fn end(&mut self) -> f64 {
        self.read_now()
            .map_or(0.0, |now| now.wrapping_sub(self.start) as f64)
    }
}

impl Drop for Kpc {
    fn drop(&mut self) {
        // SAFETY: switches counting off and gives the counters back; both take
        // plain integers and tolerate being called when already off.
        unsafe {
            (self.api.set_counting)(0);
            (self.api.set_thread_counting)(0);
            (self.api.force_all_ctrs_set)(0);
        }
    }
}
