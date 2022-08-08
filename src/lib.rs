#![feature(thread_local)]
#![cfg_attr(feature = "current_thread_id", feature(current_thread_id))]
use std::panic::catch_unwind;

use criterion::*;
/* aarch64-apple-darwin
ThreadId::current()               time:   [960.59 ps 962.20 ps 964.00 ps]
std::thread::current().id()       time:   [9.7884 ns 9.8084 ns 9.8270 ns]
core::arch::asm! thread id        time:   [333.13 ps 333.95 ps 334.75 ps]
thread_local cache current().id() time:   [972.45 ps 974.64 ps 976.86 ps]
thread_id crate                   time:   [1.9546 ns 1.9604 ns 1.9658 ns]
pointer to thread_local!          time:   [982.80 ps 987.58 ps 993.38 ps]
pointer to #[thread_local]        time:   [957.50 ps 960.68 ps 964.12 ps]
pthread_self                      time:   [1.9226 ns 1.9283 ns 1.9341 ns]
*/

thread_local! {
    static BYTE: u8 = const { 0 };
    static LAZY_THREAD_ID: std::thread::ThreadId = std::thread::current().id();
}

#[thread_local]
static BYTE2: u8 = 0;

pub fn thread_id_benches(c: &mut Criterion) {
    #[cfg(feature = "current_thread_id")]
    c.bench_function("ThreadId::current()", |b| {
        b.iter(|| std::thread::ThreadId::current());
    });
    c.bench_function("core::arch::asm! thread id", |b| {
        b.iter(|| unsafe { asm_thread_id() })
    });
    c.bench_function("std::thread::current().id()", |b| {
        b.iter(|| std::thread::current().id());
    });
    c.bench_function("thread_local cache current().id()", |b| {
        b.iter(|| LAZY_THREAD_ID.with(|id| *id));
    });
    c.bench_function("thread_id crate", |b| b.iter(|| thread_id::get()));
    c.bench_function("pointer to thread_local!", |b| {
        b.iter(|| BYTE.with(|b| b as *const u8 as usize));
    });
    c.bench_function("pointer to #[thread_local]", |b| {
        b.iter(|| (&BYTE2) as *const u8 as usize)
    });
    #[cfg(unix)]
    c.bench_function("pthread_self", |b| {
        b.iter(|| unsafe { libc::pthread_self() as usize })
    });
    #[cfg(target_os = "linux")]
    c.bench_function("gettid (linux)", |b| {
        b.iter(|| unsafe {
            libc::syscall(libc::SYS_gettid) as usize
            // libc::gettid() as usize
        })
    });
}

/// WARNING: this is not nearly tested enough def has bugs. Don't use for real,
#[allow(unused_assignments)]
#[inline]
unsafe fn asm_thread_id() -> usize {
    let mut output = 0usize;

    cfg_if::cfg_if! {
        if #[cfg(all(target_os = "macos", target_arch = "x86_64"))] {
            std::arch::asm!(
                "mov {}, gs:0",
                out(reg) output,
                options(nostack, readonly, preserves_flags),
            );
        } else if #[cfg(all(target_os = "macos", target_arch = "x86"))] {
            std::arch::asm!(
                "mov {}, fs:0",
                out(reg) output,
                options(nostack, readonly, preserves_flags),
            );
        } else if #[cfg(all(target_vendor = "apple", target_arch = "aarch64"))] {
            std::arch::asm!(
                "mrs {}, tpidrro_el0",
                out(reg) output,
                options(nostack, readonly, preserves_flags),
            );
            // 3 bits used for cpu number?
            output &= !7;
        } else if #[cfg(all(windows, target_arch = "x86_64"))] {
            std::arch::asm!(
                "mov {}, gs:48",
                out(reg) output,
                options(nostack, readonly, preserves_flags),
            );
        } else if #[cfg(all(windows, target_arch = "x86"))] {
            std::arch::asm!(
                "mov {}, fs:24",
                out(reg) output,
                options(nostack, readonly, preserves_flags),
            );
        } else if #[cfg(all(windows, target_arch = "aarch64"))] {
            std::arch::asm!(
                "mov {}, x18",
                out(reg) output,
                options(nostack, readonly, preserves_flags),
            );
        } else if #[cfg(all(windows, target_arch = "arm", any()))] {
            todo!(); // who knows???
            // #[link(name = "ntdll", kind = "dylib")]
            // extern "system" {
            //     fn NtCurrentTeb() -> *mut core::ffi::c_void;
            // }
            // NtCurrentTeb() as usize
        } else if #[cfg(any(all(target_arch = "x86_64", target_os = "linux")))] {
            std::arch::asm!(
                "mov {}, fs:0",
                out(reg) output,
                options(nostack, readonly, preserves_flags)
            );
        }
        // untested!
        else if #[cfg(all(target_arch = "x86", target_os = "linux"))] {
            // Maybe???
            std::arch::asm!(
                "mov {}, gs:0",
                out(reg) output,
                options(nostack, readonly, preserves_flags)
            );
        }
        // also untested!
        else if #[cfg(all(target_arch = "aarch64", any(target_os = "android", target_os = "linux")))] {
            type VoidP = *const core::ffi::c_void;
            let mut tpidr: *const VoidP = core::ptr::null();
            std::arch::asm!(
                "mrs {}, tpidr_el0",
                out(reg) output,
                options(nostack, readonly, preserves_flags)
            );
            let align_mask = core::mem::align_of::<VoidP>() - 1;
            debug_assert!(!tpidr.is_null() && 0 == (tpidr as usize & align_mask));
            #[cfg(target_os = "android")]
            output = tpidr.add(1).read() as usize;
            #[cfg(target_os = "linux")]
            output = tpidr.read() as usize;
        }
        // still untested!
        else if #[cfg(all(target_arch = "arm", any(target_os = "android", target_os = "linux")))] {
            type VoidP = *const core::ffi::c_void;
            let mut tpidr: *const VoidP = core::ptr::null();
            std::arch::asm!(
                "mrc p15, 0, {0}, c13, c0, 3",
                "bic {0}, {0}, #3",
                out(reg) output,
                options(nostack, readonly, preserves_flags),
            );
            let align_mask = core::mem::align_of::<VoidP>() - 1;
            debug_assert!(!tpidr.is_null() && 0 == (tpidr as usize & align_mask));
            #[cfg(target_os = "android")]
            output = tpidr.add(1).read() as usize;
            #[cfg(target_os = "linux")]
            output = tpidr.read() as usize;
        } else {
            panic!();
        }
    }
    output
}
