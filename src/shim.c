#include <stddef.h>

#if defined(_WIN32)
# define WIN32_LEAN_AND_MEAN
# include <windows.h>

unsigned char have_thread_id_shim(void) {
    return 1;
}

size_t thread_id_shim(void) {
  // Windows: works on Intel and ARM in both 32- and 64-bit
  return (size_t)NtCurrentTeb();
}

#elif defined(__GNUC__) && (defined(__x86_64__) || defined(__i386__) || defined(__arm__) || defined(__aarch64__))

// TLS register on x86 is in the FS or GS register, see: https://akkadia.org/drepper/tls.pdf
static inline void* read_tls_slot(size_t slot) {
  void* res;
  const size_t ofs = (slot*sizeof(void*));
#if defined(__i386__)
  __asm__("movl %%gs:%1, %0" : "=r" (res) : "m" (*((void**)ofs)) : );  // 32-bit always uses GS
#elif defined(__MACH__) && defined(__x86_64__)
  __asm__("movq %%gs:%1, %0" : "=r" (res) : "m" (*((void**)ofs)) : );  // x86_64 macOSX uses GS
#elif defined(__x86_64__)
    if (sizeof(void*) == 4) {
        __asm__("movl %%fs:%1, %0" : "=r" (res) : "m" (*((void**)ofs)) : );  // x32 ABI
    } else {
        __asm__("movq %%fs:%1, %0" : "=r" (res) : "m" (*((void**)ofs)) : );  // x86_64 Linux, BSD uses FS
    }
#elif defined(__arm__)
  void** tcb; UNUSED(ofs);
  __asm__ volatile ("mrc p15, 0, %0, c13, c0, 3\nbic %0, %0, #3" : "=r" (tcb));
  res = tcb[slot];
#elif defined(__aarch64__)
  void** tcb; UNUSED(ofs);
#if defined(__APPLE__)
  __asm__ volatile ("mrs %0, tpidrro_el0" : "=r" (tcb));
#else
  __asm__ volatile ("mrs %0, tpidr_el0" : "=r" (tcb));
#endif
  res = tcb[slot];
#endif
  return res;
}

unsigned char have_thread_id_shim(void) {
    return 1;
}
size_t thread_id_shim(void) {
    return (size_t)read_tls_slot(0);
}
#else

unsigned char have_thread_id_shim(void) {
    return 0;
}
size_t thread_id_shim(void) {
    return 0;
}
#endif
