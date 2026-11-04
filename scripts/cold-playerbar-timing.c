// SPDX-License-Identifier: GPL-3.0-or-later
// Bounded synthetic-fixture diagnostic only. No URI, object, widget name,
// credential, file path or caller arguments are written to the trace.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>
static int out = -1;
static atomic_long gtk_tid;
static __thread int iteration_depth;
static int64_t micros(clockid_t clock) {
    struct timespec t;
    clock_gettime(clock, &t);
    return (int64_t)t.tv_sec * 1000000 + t.tv_nsec / 1000;
}
static void record(const char *stage, int64_t start, int64_t cpu_start) {
    if (out < 0) return;
    int64_t end = micros(CLOCK_MONOTONIC);
    int64_t cpu = micros(CLOCK_THREAD_CPUTIME_ID) - cpu_start;
    long tid = syscall(SYS_gettid);
    char line[320];
    int n = snprintf(line, sizeof line,
        "{\"stage\":\"%s\",\"start_us\":%lld,\"end_us\":%lld,\"elapsed_us\":%lld,\"thread_cpu_us\":%lld,\"gtk_thread\":%s,\"tid\":%ld}\n",
        stage, (long long)start, (long long)end, (long long)(end-start),
        (long long)cpu, tid == atomic_load_explicit(&gtk_tid, memory_order_relaxed) ? "true" : "false", tid);
    if (n > 0 && n < (int)sizeof line) syscall(SYS_write, out, line, (size_t)n);
}
__attribute__((constructor)) static void trace_init(void) {
    const char *file = getenv("KARIN_COLD_TRACE_FILE");
    if (file) out = open(file, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0600);
    // Do not instrument the fixture's window manager or input subprocesses.
    unsetenv("LD_PRELOAD");
    unsetenv("KARIN_COLD_TRACE_FILE");
    if (out < 0) {
        static const char message[] = "Karin cold observer: trace-open-failed\n";
        syscall(SYS_write, STDERR_FILENO, message, sizeof message - 1);
        _exit(125);
    }
}
#define START int64_t begin = micros(CLOCK_MONOTONIC), cpu = micros(CLOCK_THREAD_CPUTIME_ID)
int gtk_init_check(void) {
    static __thread int (*real)(void);
    if (!real) real = dlsym(RTLD_NEXT, "gtk_init_check");
    atomic_store_explicit(&gtk_tid, syscall(SYS_gettid), memory_order_relaxed);
    START; int result = real(); record("gtk-init", begin, cpu); return result;
}
int g_main_context_iteration(void *context, int may_block) {
    static __thread int (*real)(void *, int);
    if (!real) real = dlsym(RTLD_NEXT, "g_main_context_iteration");
    START; iteration_depth++; int result = real(context, may_block); iteration_depth--;
    if (!iteration_depth && micros(CLOCK_MONOTONIC) - begin >= 5000)
        record("main-context-iteration", begin, cpu);
    return result;
}
void gtk_revealer_set_reveal_child(void *revealer, int reveal) {
    static __thread void (*real)(void *, int);
    static __thread const char *(*get_name)(void *);
    static __thread int (*get_reveal)(void *);
    if (!real) real = dlsym(RTLD_NEXT, "gtk_revealer_set_reveal_child");
    if (!get_name) get_name = dlsym(RTLD_NEXT, "gtk_widget_get_name");
    if (!get_reveal) get_reveal = dlsym(RTLD_NEXT, "gtk_revealer_get_reveal_child");
    int target = get_name && !strcmp(get_name(revealer), "music-player-revealer");
    int changed = target && get_reveal && !!get_reveal(revealer) != !!reveal;
    START; real(revealer, reveal);
    if (changed) record(reveal ? "footer-request-open" : "footer-request-close", begin, cpu);
}
void *gdk_memory_texture_new(int width, int height, int format, void *bytes, size_t stride) {
    static __thread void *(*real)(int, int, int, void *, size_t);
    if (!real) real = dlsym(RTLD_NEXT, "gdk_memory_texture_new");
    START; void *result = real(width, height, format, bytes, stride);
    record("memory-texture-create", begin, cpu); return result;
}
void *gdk_texture_new_for_pixbuf(void *pixbuf) {
    static __thread void *(*real)(void *);
    if (!real) real = dlsym(RTLD_NEXT, "gdk_texture_new_for_pixbuf");
    START; void *result = real(pixbuf); record("pixbuf-texture-create", begin, cpu); return result;
}
void gsk_renderer_render(void *renderer, void *root, const void *region) {
    static __thread void (*real)(void *, void *, const void *);
    if (!real) real = dlsym(RTLD_NEXT, "gsk_renderer_render");
    START; real(renderer, root, region); record("gsk-render", begin, cpu);
}
void *gsk_renderer_render_texture(void *renderer, void *root, const void *viewport) {
    static __thread void *(*real)(void *, void *, const void *);
    if (!real) real = dlsym(RTLD_NEXT, "gsk_renderer_render_texture");
    START; void *result = real(renderer, root, viewport);
    record("gsk-render-texture", begin, cpu); return result;
}
int gst_init_check(int *argc, char ***argv, void **error) {
    static __thread int (*real)(int *, char ***, void **);
    if (!real) real = dlsym(RTLD_NEXT, "gst_init_check");
    START; int result = real(argc, argv, error); record("gst-init", begin, cpu); return result;
}
void *gst_element_factory_make(const char *factory, const char *name) {
    static __thread void *(*real)(const char *, const char *);
    if (!real) real = dlsym(RTLD_NEXT, "gst_element_factory_make");
    START; void *result = real(factory, name); record("gst-element-create", begin, cpu); return result;
}
int gst_element_set_state(void *element, int state) {
    static __thread int (*real)(void *, int);
    if (!real) real = dlsym(RTLD_NEXT, "gst_element_set_state");
    START; int result = real(element, state); record("gst-element-state", begin, cpu); return result;
}
// The current software-Mesa gallium library imports these two LLVM C APIs.
// No shader/module contents, names or pointers are recorded.
static void *llvm_symbol(const char *name) {
    void *result = dlsym(RTLD_NEXT, name);
    if (!result) {
        void *module = dlopen("libLLVM.so.19.1", RTLD_LAZY | RTLD_NOLOAD);
        if (module) result = dlsym(module, name);
    }
    if (!result) {
        START; record("symbol-unavailable", begin, cpu);
        _exit(125);
    }
    return result;
}
void *LLVMRunPasses(void *module, const char *passes, void *machine, void *options) {
    static __thread void *(*real)(void *, const char *, void *, void *);
    if (!real) real = llvm_symbol("LLVMRunPasses");
    START; void *result = real(module, passes, machine, options);
    record("llvm-run-passes", begin, cpu); return result;
}
void *LLVMGetPointerToGlobal(void *engine, void *global) {
    static __thread void *(*real)(void *, void *);
    if (!real) real = llvm_symbol("LLVMGetPointerToGlobal");
    START; void *result = real(engine, global);
    record("llvm-compile-global", begin, cpu); return result;
}
