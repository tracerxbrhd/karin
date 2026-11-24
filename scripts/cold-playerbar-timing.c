// SPDX-License-Identifier: GPL-3.0-or-later
// Opt-in synthetic-fixture diagnostic. Never record caller arguments, widget
// names, paths, URIs, credentials, shader contents or absolute addresses.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <execinfo.h>
#include <fcntl.h>
#include <link.h>
#include <stdint.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

#define MAX_ROWS 50000
#define MAX_STACKS 4
#define MAX_FRAMES 24
static int out = -1, evidence = -1;
static atomic_long gtk_tid;
static atomic_uint rows, stacks, footer_opens, row_signal_id, row_records;
static __thread unsigned iteration_depth, layout_depth, snapshot_depth, row_depth, gst_factory_depth;

static int64_t micros(clockid_t clock) {
    struct timespec t;
    clock_gettime(clock, &t);
    return (int64_t)t.tv_sec * 1000000 + t.tv_nsec / 1000;
}
static int gtk_thread(void) {
    return syscall(SYS_gettid) == atomic_load_explicit(&gtk_tid, memory_order_relaxed);
}
static void fail(const char *message, size_t length) {
    syscall(SYS_write, STDERR_FILENO, message, length);
    _exit(125);
}
#define FAIL(message) fail(message "\n", sizeof(message "\n") - 1)
static void write_line(int fd, const char *line, size_t length) {
    if (fd < 0) return;
    ssize_t n;
    do n = syscall(SYS_write, fd, line, length); while (n < 0 && errno == EINTR);
    // One O_APPEND write keeps concurrent records whole. Incomplete evidence
    // fails the diagnostic instead of being treated as missing coverage.
    if (n != (ssize_t)length) FAIL("Karin cold observer: trace-write-failed");
}
static void record(const char *stage, int64_t start, int64_t cpu_start) {
    if (out < 0) return;
    if (atomic_fetch_add_explicit(&rows, 1, memory_order_relaxed) >= MAX_ROWS)
        FAIL("Karin cold observer: trace-row-limit");
    int64_t end = micros(CLOCK_MONOTONIC);
    int64_t cpu = micros(CLOCK_THREAD_CPUTIME_ID) - cpu_start;
    char line[320];
    int n = snprintf(line, sizeof line,
        "{\"stage\":\"%s\",\"start_us\":%lld,\"end_us\":%lld,\"elapsed_us\":%lld,\"thread_cpu_us\":%lld,\"gtk_thread\":%s,\"tid\":%ld}\n",
        stage, (long long)start, (long long)end, (long long)(end-start),
        (long long)cpu, gtk_thread() ? "true" : "false", syscall(SYS_gettid));
    if (n <= 0 || n >= (int)sizeof line) FAIL("Karin cold observer: trace-row-overflow");
    write_line(out, line, (size_t)n);
}
static int new_file(const char *name) {
    const char *file = getenv(name);
    return file ? open(file, O_WRONLY | O_APPEND | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0600) : -1;
}
__attribute__((constructor)) static void trace_init(void) {
    out = new_file("KARIN_COLD_TRACE_FILE");
    evidence = new_file("KARIN_COLD_EVIDENCE_FILE");
    // The fixture's window manager/input subprocesses must not inherit this.
    unsetenv("LD_PRELOAD");
    unsetenv("KARIN_COLD_TRACE_FILE");
    unsetenv("KARIN_COLD_EVIDENCE_FILE");
    if (out < 0 || evidence < 0) FAIL("Karin cold observer: trace-open-failed");
}
#define START int64_t begin = micros(CLOCK_MONOTONIC), cpu = micros(CLOCK_THREAD_CPUTIME_ID)

// Evidence strings come only from loaded-library symbols or the current GL
// driver. Bound and JSON-escape them; no raw addresses or file paths are emitted.
static void json_string(char *dest, size_t size, const char *source) {
    size_t used = 0;
    if (!source) {
        snprintf(dest, size, "null");
        return;
    }
    dest[used++] = '"';
    for (size_t i = 0; source[i] && i < 192 && used + 7 < size; ++i) {
        unsigned char c = (unsigned char)source[i];
        if (c == '"' || c == '\\') {
            dest[used++] = '\\'; dest[used++] = (char)c;
        } else if (c < 0x20 || c >= 0x7f) {
            used += (size_t)snprintf(dest + used, size - used, "\\u%04x", c);
        } else dest[used++] = (char)c;
    }
    dest[used++] = '"'; dest[used] = '\0';
}
static const char *basename_only(const char *path) {
    const char *slash = path ? strrchr(path, '/') : NULL;
    return slash ? slash + 1 : path;
}
static void stack_evidence(const char *stage, int64_t elapsed) {
    if (!gtk_thread() || !atomic_load_explicit(&footer_opens, memory_order_relaxed) || elapsed < 5000)
        return;
    if (atomic_fetch_add_explicit(&stacks, 1, memory_order_relaxed) >= MAX_STACKS) return;
    START;
    void *frames[MAX_FRAMES];
    int count = backtrace(frames, MAX_FRAMES);
    char line[16384];
    size_t used = (size_t)snprintf(line, sizeof line,
        "{\"type\":\"llvm-stack\",\"stage\":\"%s\",\"tid\":%ld,\"gtk_thread\":true,\"capture_time_us\":%lld,\"stack\":[",
        stage, syscall(SYS_gettid), (long long)begin);
    // Captured after the measured call: this is its still-live caller chain,
    // not a sampled stack inside an LLVM optimization pass.
    for (int i = 1; i < count; ++i) {
        Dl_info info = {0};
        dladdr(frames[i], &info);
        char module[1200], symbol[1200];
        json_string(module, sizeof module, info.dli_fname ? basename_only(info.dli_fname) : "unknown");
        json_string(symbol, sizeof symbol, info.dli_sname);
        uintptr_t offset = info.dli_fbase ? (uintptr_t)frames[i] - (uintptr_t)info.dli_fbase : 0;
        used += (size_t)snprintf(line + used, sizeof line - used,
            "%s{\"module\":%s,\"symbol\":%s,\"offset\":%llu}",
            i == 1 ? "" : ",", module, symbol, (unsigned long long)offset);
        if (used + 2500 >= sizeof line) break;
    }
    used += (size_t)snprintf(line + used, sizeof line - used, "]}\n");
    write_line(evidence, line, used);
    record("observer-stack-capture", begin, cpu);
}
struct FindSymbol { const char *name, *prefix; void *result; };
static int find_loaded_symbol(struct dl_phdr_info *info, size_t size, void *data) {
    (void)size;
    struct FindSymbol *find = data;
    const char *base = basename_only(info->dlpi_name);
    if (!base || strncmp(base, find->prefix, strlen(find->prefix))) return 0;
    void *module = dlopen(info->dlpi_name, RTLD_LAZY | RTLD_NOLOAD);
    if (module) {
        find->result = dlsym(module, find->name);
        dlclose(module);
    }
    return find->result != NULL;
}
static void *loaded_symbol(const char *name, const char *prefix) {
    void *result = dlsym(RTLD_NEXT, name);
    if (!result) {
        // Inspect only a library already loaded by the actual driver. Never
        // load a second LLVM/GL implementation or require a distro soname.
        struct FindSymbol find = {name, prefix, NULL};
        dl_iterate_phdr(find_loaded_symbol, &find);
        result = find.result;
    }
    return result;
}
static void *llvm_symbol(const char *name) {
    void *result = loaded_symbol(name, "libLLVM");
    if (!result) FAIL("Karin cold observer: llvm-symbol-unavailable");
    return result;
}
static void renderer_evidence(const char *stage, void *renderer) {
    static __thread int seen;
    if (!gtk_thread() || seen) return;
    const char *(*type_name)(void *) = dlsym(RTLD_DEFAULT, "g_type_name_from_instance");
    if (!type_name) return;
    char name[1200], line[1536];
    json_string(name, sizeof name, type_name(renderer));
    int n = snprintf(line, sizeof line,
        "{\"type\":\"gsk-renderer\",\"stage\":\"%s\",\"tid\":%ld,\"gtk_thread\":true,\"capture_time_us\":%lld,\"renderer\":%s}\n",
        stage, syscall(SYS_gettid), (long long)micros(CLOCK_MONOTONIC), name);
    if (n <= 0 || n >= (int)sizeof line) FAIL("Karin cold observer: renderer-row-overflow");
    write_line(evidence, line, (size_t)n);
    seen = 1;
}
static void gl_evidence(const char *stage) {
    static __thread void *seen[4];
    static __thread unsigned seen_count;
    if (!gtk_thread() || seen_count == 4) return;
    void *(*egl_current)(void) = loaded_symbol("eglGetCurrentContext", "libEGL");
    void *(*glx_current)(void) = loaded_symbol("glXGetCurrentContext", "libGLX");
    void *context = egl_current ? egl_current() : NULL;
    if (!context && glx_current) context = glx_current();
    if (!context) return;
    for (unsigned i = 0; i < seen_count; ++i) if (seen[i] == context) return;
    const unsigned char *(*get_string)(unsigned) = loaded_symbol("glGetString", "libGL");
    if (!get_string) {
        void *(*get_proc)(const char *) = loaded_symbol("eglGetProcAddress", "libEGL");
        if (get_proc) get_string = get_proc("glGetString");
    }
    if (!get_string) return;
    const char *vendor = (const char *)get_string(0x1f00);
    const char *renderer = (const char *)get_string(0x1f01);
    const char *version = (const char *)get_string(0x1f02);
    if (!vendor || !renderer || !version) return;
    seen[seen_count++] = context;
    char a[1200], b[1200], c[1200], line[4096];
    json_string(a, sizeof a, vendor); json_string(b, sizeof b, renderer); json_string(c, sizeof c, version);
    int n = snprintf(line, sizeof line,
        "{\"type\":\"gl-context\",\"stage\":\"%s\",\"tid\":%ld,\"gtk_thread\":true,\"capture_time_us\":%lld,\"vendor\":%s,\"renderer\":%s,\"version\":%s}\n",
        stage, syscall(SYS_gettid), (long long)micros(CLOCK_MONOTONIC), a, b, c);
    if (n <= 0 || n >= (int)sizeof line) FAIL("Karin cold observer: context-row-overflow");
    write_line(evidence, line, (size_t)n);
}

static void probe_gl_context(const char *stage) {
    static __thread unsigned attempts;
    if (!gtk_thread() || !atomic_load_explicit(&footer_opens, memory_order_relaxed) || attempts == 4) return;
    ++attempts;
    // LLVM hooks are inside the actual driver call on the GTK thread. Query
    // only that already-current context; never create or make one current.
    START; gl_evidence(stage); record("observer-context-query", begin, cpu);
}

int gtk_init_check(void) {
    static __thread int (*real)(void);
    if (!real) real = dlsym(RTLD_NEXT, "gtk_init_check");
    atomic_store_explicit(&gtk_tid, syscall(SYS_gettid), memory_order_relaxed);
    START; int result = real(); record("gtk-init", begin, cpu); return result;
}
// The canonical va_list API preserves the arbitrary signal arguments and
// return-value slot. Only the fixed row-activated signal is timed, four times;
// no object properties, signal arguments or unrelated emissions are logged.
static void emit_valist(void *instance, unsigned signal, unsigned detail, va_list args) {
    static __thread void (*real)(void *, unsigned, unsigned, va_list);
    static __thread const char *(*name)(unsigned);
    if (!real) real = dlsym(RTLD_NEXT, "g_signal_emit_valist");
    if (!name) name = dlsym(RTLD_NEXT, "g_signal_name");
    unsigned known = atomic_load_explicit(&row_signal_id, memory_order_relaxed);
    int target = known && signal == known;
    if (!known) {
        const char *value = name(signal);
        target = value && !strcmp(value, "row-activated");
        if (target) atomic_store_explicit(&row_signal_id, signal, memory_order_relaxed);
    }
    int observe = target && !row_depth && atomic_load_explicit(&row_records, memory_order_relaxed) < 4;
    int64_t begin = observe ? micros(CLOCK_MONOTONIC) : 0;
    int64_t cpu = observe ? micros(CLOCK_THREAD_CPUTIME_ID) : 0;
    row_depth += target;
    real(instance, signal, detail, args);
    row_depth -= target;
    if (observe && atomic_fetch_add_explicit(&row_records, 1, memory_order_relaxed) < 4)
        record("app-row-activation", begin, cpu);
}
void g_signal_emit_valist(void *instance, unsigned signal, unsigned detail, va_list args) {
    emit_valist(instance, signal, detail, args);
}
void g_signal_emit(void *instance, unsigned signal, unsigned detail, ...) {
    va_list args;
    va_start(args, detail);
    emit_valist(instance, signal, detail, args);
    va_end(args);
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
    if (changed && reveal) atomic_fetch_add_explicit(&footer_opens, 1, memory_order_relaxed);
    START; real(revealer, reveal);
    if (changed) record(reveal ? "footer-request-open" : "footer-request-close", begin, cpu);
}
void gtk_widget_measure(void *widget, int orientation, int for_size, int *minimum, int *natural,
                        int *minimum_baseline, int *natural_baseline) {
    static __thread void (*real)(void *, int, int, int *, int *, int *, int *);
    if (!real) real = dlsym(RTLD_NEXT, "gtk_widget_measure");
    START; layout_depth++; real(widget, orientation, for_size, minimum, natural, minimum_baseline, natural_baseline); layout_depth--;
    if (!layout_depth && micros(CLOCK_MONOTONIC) - begin >= 100) record("gtk-layout-measure", begin, cpu);
}
void gtk_widget_allocate(void *widget, int width, int height, int baseline, void *transform) {
    static __thread void (*real)(void *, int, int, int, void *);
    if (!real) real = dlsym(RTLD_NEXT, "gtk_widget_allocate");
    START; layout_depth++; real(widget, width, height, baseline, transform); layout_depth--;
    if (!layout_depth && micros(CLOCK_MONOTONIC) - begin >= 100) record("gtk-layout-allocate", begin, cpu);
}
void gtk_widget_size_allocate(void *widget, const void *allocation, int baseline) {
    static __thread void (*real)(void *, const void *, int);
    if (!real) real = dlsym(RTLD_NEXT, "gtk_widget_size_allocate");
    START; layout_depth++; real(widget, allocation, baseline); layout_depth--;
    if (!layout_depth && micros(CLOCK_MONOTONIC) - begin >= 100) record("gtk-layout-allocate", begin, cpu);
}
void gtk_widget_snapshot_child(void *widget, void *child, void *snapshot) {
    static __thread void (*real)(void *, void *, void *);
    if (!real) real = dlsym(RTLD_NEXT, "gtk_widget_snapshot_child");
    START; snapshot_depth++; real(widget, child, snapshot); snapshot_depth--;
    if (!snapshot_depth && micros(CLOCK_MONOTONIC) - begin >= 100) record("gtk-snapshot-child", begin, cpu);
}
void *gtk_snapshot_to_node(void *snapshot) {
    static __thread void *(*real)(void *);
    if (!real) real = dlsym(RTLD_NEXT, "gtk_snapshot_to_node");
    START; void *result = real(snapshot); record("gtk-snapshot-to-node", begin, cpu); return result;
}
void *gtk_snapshot_free_to_node(void *snapshot) {
    static __thread void *(*real)(void *);
    if (!real) real = dlsym(RTLD_NEXT, "gtk_snapshot_free_to_node");
    START; void *result = real(snapshot); record("gtk-snapshot-to-node", begin, cpu); return result;
}
void gdk_gl_context_make_current(void *context) {
    static __thread void (*real)(void *);
    if (!real) real = dlsym(RTLD_NEXT, "gdk_gl_context_make_current");
    START; real(context); record("gdk-gl-make-current", begin, cpu); gl_evidence("gdk-gl-make-current");
}
int eglMakeCurrent(void *display, void *draw, void *read, void *context) {
    static __thread int (*real)(void *, void *, void *, void *);
    if (!real) real = loaded_symbol("eglMakeCurrent", "libEGL");
    int result = real(display, draw, read, context);
    if (result && context) gl_evidence("egl-make-current");
    return result;
}
int glXMakeCurrent(void *display, unsigned long drawable, void *context) {
    static __thread int (*real)(void *, unsigned long, void *);
    if (!real) real = loaded_symbol("glXMakeCurrent", "libGLX");
    int result = real(display, drawable, context);
    if (result && context) gl_evidence("glx-make-current");
    return result;
}
int glXMakeContextCurrent(void *display, unsigned long draw, unsigned long read, void *context) {
    static __thread int (*real)(void *, unsigned long, unsigned long, void *);
    if (!real) real = loaded_symbol("glXMakeContextCurrent", "libGLX");
    int result = real(display, draw, read, context);
    if (result && context) gl_evidence("glx-make-current");
    return result;
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
    renderer_evidence("gsk-render", renderer);
    START; real(renderer, root, region); record("gsk-render", begin, cpu);
}
void *gsk_renderer_render_texture(void *renderer, void *root, const void *viewport) {
    static __thread void *(*real)(void *, void *, const void *);
    if (!real) real = dlsym(RTLD_NEXT, "gsk_renderer_render_texture");
    renderer_evidence("gsk-render-texture", renderer);
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
    START; gst_factory_depth++; void *result = real(factory, name); gst_factory_depth--;
    if (!gst_factory_depth) record("gst-element-create", begin, cpu);
    return result;
}
void *gst_element_factory_create_with_properties(void *factory, unsigned n, const char **names, const void *values) {
    static __thread void *(*real)(void *, unsigned, const char **, const void *);
    if (!real) real = dlsym(RTLD_NEXT, "gst_element_factory_create_with_properties");
    START; gst_factory_depth++; void *result = real(factory, n, names, values); gst_factory_depth--;
    if (!gst_factory_depth) record("gst-element-create", begin, cpu);
    return result;
}
void *gst_element_factory_make_with_properties(const char *factory, unsigned n, const char **names, const void *values) {
    static __thread void *(*real)(const char *, unsigned, const char **, const void *);
    if (!real) real = dlsym(RTLD_NEXT, "gst_element_factory_make_with_properties");
    START; gst_factory_depth++; void *result = real(factory, n, names, values); gst_factory_depth--;
    if (!gst_factory_depth) record("gst-element-create", begin, cpu);
    return result;
}
int gst_element_set_state(void *element, int state) {
    static __thread int (*real)(void *, int);
    if (!real) real = dlsym(RTLD_NEXT, "gst_element_set_state");
    START; int result = real(element, state); record("gst-element-state", begin, cpu); return result;
}
void *LLVMRunPasses(void *module, const char *passes, void *machine, void *options) {
    static __thread void *(*real)(void *, const char *, void *, void *);
    if (!real) real = llvm_symbol("LLVMRunPasses");
    START; void *result = real(module, passes, machine, options);
    record("llvm-run-passes", begin, cpu); probe_gl_context("llvm-run-passes"); stack_evidence("llvm-run-passes", micros(CLOCK_MONOTONIC) - begin); return result;
}
void *LLVMGetPointerToGlobal(void *engine, void *global) {
    static __thread void *(*real)(void *, void *);
    if (!real) real = llvm_symbol("LLVMGetPointerToGlobal");
    START; void *result = real(engine, global);
    record("llvm-compile-global", begin, cpu); probe_gl_context("llvm-compile-global"); stack_evidence("llvm-compile-global", micros(CLOCK_MONOTONIC) - begin); return result;
}
