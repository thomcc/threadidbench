#![feature(asm, thread_local, once_cell)]//, core_intrinsics)]
extern crate criterion;
// use core::intrinsics::unlikely;
use std::{lazy::SyncLazy};
use criterion::*;
use libc::pthread_getspecific;
// use core::hint::black_box;
// #[cfg(not(feature = "no-c"))]
extern {
    fn have_thread_id_shim() -> u8;
    fn thread_id_shim() -> u8;
}

extern {
    #[cfg(not(target_os = "dragonfly"))]
    #[cfg_attr(any(target_os = "macos",
                   target_os = "ios",
                   target_os = "freebsd"),
               link_name = "__error")]
    #[cfg_attr(any(target_os = "openbsd",
                   target_os = "netbsd",
                   target_os = "bitrig",
                   target_os = "android"),
               link_name = "__errno")]
    #[cfg_attr(any(target_os = "solaris",
                   target_os = "illumos"),
               link_name = "___errno")]
    #[cfg_attr(target_os = "linux",
               link_name = "__errno_location")]
    fn errno_location() -> *mut libc::c_int;
}

std::thread_local! { static TLMAC_BYTE: u8 = 0; }
#[thread_local] static TLATTR_BYTE: u8 = 0;

#[inline]
fn tid_tlsaddr_macro() -> usize {
    TLMAC_BYTE.with(|v| v as *const _ as usize)
}

#[inline]
fn tid_tlsaddr_attr() -> usize {
    &TLATTR_BYTE as *const _ as usize
}

#[inline]
fn tid_errno() -> usize {
    unsafe { errno_location() as usize }
}

#[inline]
fn tid_std_thread() -> std::thread::ThreadId {
    std::thread::current().id()
}

// unsigned* cpu, unsigned* node, void* unused
type GetCpu = extern "C" fn(
    cpu: *mut libc::c_uint,
    node: *mut libc::c_uint,
    _unused: *mut core::ffi::c_void
) -> libc::c_int;
#[cfg(unix)]
unsafe fn get_getcpu() -> Option<GetCpu> {
    if cfg!(target_os = "linux") {
        let h = libc::dlopen("linux-vdso.so.1\0".as_ptr().cast(), libc::RTLD_LAZY | libc::RTLD_LOCAL | libc::RTLD_NOLOAD);
        if h.is_null() {
            return None;
        }
        let r = libc::dlsym(h, "__vdso_getcpu\0".as_ptr().cast());
        if r.is_null() {
            return None;
        }
        Some(core::mem::transmute(r))
    } else {
        None
    }
}

#[cfg(not(unix))]
fn get_getcpu() -> Option<GetCpu> { None }

#[inline]
fn getcpu(f: GetCpu) -> (u32, u32) {
    let mut core = 0;
    let mut node = 0;
    f(&mut core, &mut node, core::ptr::null_mut());
    (core, node)
}

// TLS register on x86 is in the FS or GS register, see: https://akkadia.org/drepper/tls.pdf
#[cfg(target_vendor = "apple")]
#[inline]
#[allow(unused_assignments)]
fn asm_thread_id() -> usize {
    let mut o = 0usize;
    #[cfg(any(
        target_arch = "x86",
        all(target_arch = "x86_64", target_vendor = "apple"),
    ))]
    unsafe {
        asm!(
            "mov {}, gs:[0]",
            out(reg) o,
            options(nostack, readonly, preserves_flags)
        );
    }
    #[cfg(all(
        target_arch = "x86_64",
        not(target_vendor = "apple"),
    ))]
    unsafe {
        asm!(
            "mov {}, fs:[0]",
            out(reg) o,
            options(nostack, readonly, preserves_flags)
        );
    }
    #[cfg(target_arch = "arm")]
    unsafe {
        asm!(
            "mrc p15, 0, {}, c13, c0, 3",
            out(reg) o,
            options(nostack, readonly),// preserves flags?
        );
        // lower 2 bits are cpu number — fixme: is this true on non-apple?
        o = o & !3;
    }
    #[cfg(all(target_arch = "aarch64", target_vendor = "apple"))]
    unsafe {
        asm!(
            "mrs {}, tpidrro_el0",
            out(reg) o,
            options(nostack, readonly),// preserves flags?
        );
    }
    #[cfg(all(target_arch = "aarch64", not(target_vendor = "apple")))]
    unsafe {
        asm!(
            "mrs {}, tpidr_el0",
            out(reg) o,
            options(nostack, readonly),// preserves flags?
        );
    }
    o
}

#[cfg(all(windows, any(target_arch = "x86", target_arch = "x86_64")))]
#[inline]
fn asm_thread_id() -> usize {
    let mut o = 0usize;
    #[cfg(target_arch = "x86")]
    unsafe {
        asm!(
            "mov {}, gs:[{}]",
            out(reg) o,
            const 0x30,
            options(nostack, readonly, preserves_flags)
        );
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        asm!(
            "mov {}, fs:[{}]",
            out(reg) o,
            const 0x18,
            options(nostack, readonly, preserves_flags)
        );
    }
    return o
}

static LAZYKEY: SyncLazy<libc::pthread_key_t> = SyncLazy::new(|| unsafe {
    let mut key = core::mem::MaybeUninit::<libc::pthread_key_t>::zeroed();
    libc::pthread_key_create(key.as_mut_ptr(), None);
    key.assume_init()
});
static KEY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn thread_id_benches(c: &mut Criterion) {
    c.bench_function("std::thread::current().id()", |b| {
        b.iter(|| tid_std_thread());
    });
    c.bench_function("thread_local! addr", |b| {
        b.iter(|| tid_tlsaddr_macro());
    });
    c.bench_function("#[thread_local] addr", |b| {
        b.iter(|| tid_tlsaddr_attr());
    });
    c.bench_function("errno address", |b| {
        b.iter(|| tid_errno());
    });
    if let Some(vdso_getcpu) = unsafe { get_getcpu() } {
        c.bench_function("vdso_getcpu", |b| {
            b.iter(|| getcpu(vdso_getcpu));
        });
    }
    #[cfg(unix)] {
        c.bench_function("pthread_self", |b| {
            b.iter(|| unsafe { libc::pthread_self() as usize });
        });
        let mut key = core::mem::MaybeUninit::<libc::pthread_key_t>::zeroed();
        unsafe { libc::pthread_key_create(key.as_mut_ptr(), None) };
        let key = unsafe { key.assume_init() };
        c.bench_function("pthread_getspecific", |b| {
            b.iter(|| unsafe { pthread_getspecific(key) });
        });
        KEY.store(key, core::sync::atomic::Ordering::Release);
        c.bench_function("pthread_getspecific acq", |b| {
            b.iter(|| unsafe { pthread_getspecific(KEY.load(std::sync::atomic::Ordering::Acquire)) });
        });
        c.bench_function("pthread_getspecific with SyncLazy key init", |b| {
            b.iter(|| unsafe { pthread_getspecific(*LAZYKEY); })
        });
    }

    c.bench_function("thread_id::get()", |b| {
        b.iter(|| thread_id::get());
    });

    #[cfg(any(
        all(windows, any(target_arch = "x86", target_arch = "x86_64")),
        target_vendor = "apple",
    ))] {
        c.bench_function("use asm!", |b| {
            b.iter(|| asm_thread_id());
        });
    }
    // #[cfg(not(feature = "no-c"))]
    {
        if unsafe { have_thread_id_shim() } != 0 {
            c.bench_function("call into inline asm in c", |b| {
                b.iter(|| black_box(unsafe { thread_id_shim() }));
            });
        }
    }
}
