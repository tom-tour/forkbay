// nx2-quiet.dll: lets NX2 Launcher apply settings to a running emulator through the emulator's own
// "Configure Current Game" window, without that window ever showing or taking the focus.
//
// Win32 only, no Qt: one build fits every emulator, whatever compiler built it and whether Qt is a DLL
// or inside the exe. NX2 Launcher has Windows load it into the emulator with two standard hooks (a
// message hook on its interface thread, a window event hook on its process); nothing is written into
// the emulator's memory from outside.
//
// It changes nothing until armed: NX2 Launcher sets the named event "Local\nx2-quiet-<pid>" only for
// the half second of one operation. While armed, each window the emulator opens over its main window
// is cloaked (Windows never composes it on screen), kept out of the taskbar and never activated, and
// the main window is never disabled (it would hand the focus to another program, and a full screen
// game would leave full screen).
#include <windows.h>
#include <psapi.h>
#include <dwmapi.h>

static const wchar_t* PROP = L"nx2-quiet";
static HMODULE self;
static volatile LONG patched;

static BOOL armed(void) {
    wchar_t name[64];
    wsprintfW(name, L"Local\\nx2-quiet-%lu", GetCurrentProcessId());
    HANDLE e = OpenEventW(SYNCHRONIZE, FALSE, name);
    if (!e) return FALSE;
    const BOOL on = WaitForSingleObject(e, 0) == WAIT_OBJECT_0;
    CloseHandle(e);
    return on;
}

static BOOL quiet(HWND w) { return w && GetPropW(w, PROP) != NULL; }

// The calls a program makes to bring a window forward or to block the one below. For a quiet window they
// show it without activating it, or do nothing: the focus stays where the player left it.
static BOOL (WINAPI* real_ShowWindow)(HWND, int);
static BOOL (WINAPI* real_SetForegroundWindow)(HWND);
static HWND (WINAPI* real_SetActiveWindow)(HWND);
static HWND (WINAPI* real_SetFocus)(HWND);
static BOOL (WINAPI* real_BringWindowToTop)(HWND);
static BOOL (WINAPI* real_EnableWindow)(HWND, BOOL);
static LONG_PTR (WINAPI* real_SetWindowLongPtrW)(HWND, int, LONG_PTR);

static BOOL WINAPI my_ShowWindow(HWND w, int cmd) {
    if (quiet(w) && cmd != SW_HIDE) cmd = SW_SHOWNOACTIVATE;
    return real_ShowWindow(w, cmd);
}
static BOOL WINAPI my_SetForegroundWindow(HWND w) { return quiet(w) ? TRUE : real_SetForegroundWindow(w); }
static HWND WINAPI my_SetActiveWindow(HWND w) { return quiet(w) ? GetActiveWindow() : real_SetActiveWindow(w); }
static HWND WINAPI my_SetFocus(HWND w) { return quiet(w) ? GetFocus() : real_SetFocus(w); }
static BOOL WINAPI my_BringWindowToTop(HWND w) { return quiet(w) ? TRUE : real_BringWindowToTop(w); }
static BOOL WINAPI my_EnableWindow(HWND w, BOOL on) {
    if (!on && armed()) return FALSE;
    return real_EnableWindow(w, on);
}
// Qt disables the window below a dialog through its style, before the dialog exists.
static LONG_PTR WINAPI my_SetWindowLongPtrW(HWND w, int index, LONG_PTR value) {
    if (index == GWL_STYLE && (value & WS_DISABLED) && !(GetWindowLongPtrW(w, GWL_STYLE) & WS_DISABLED) && armed()) {
        value &= ~(LONG_PTR)WS_DISABLED;
    }
    return real_SetWindowLongPtrW(w, index, value);
}

// Points one module's imports of user32 at the functions above. Import tables are the same whatever compiler built it.
static void patch_module(HMODULE m) {
    BYTE* base = (BYTE*)m;
    IMAGE_DOS_HEADER* dos = (IMAGE_DOS_HEADER*)base;
    if (dos->e_magic != IMAGE_DOS_SIGNATURE) return;
    IMAGE_NT_HEADERS* nt = (IMAGE_NT_HEADERS*)(base + dos->e_lfanew);
    IMAGE_DATA_DIRECTORY dir = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT];
    if (!dir.VirtualAddress) return;
    struct { void* real; void* mine; } map[] = {
        {(void*)real_ShowWindow, (void*)my_ShowWindow},
        {(void*)real_SetForegroundWindow, (void*)my_SetForegroundWindow},
        {(void*)real_SetActiveWindow, (void*)my_SetActiveWindow},
        {(void*)real_SetFocus, (void*)my_SetFocus},
        {(void*)real_BringWindowToTop, (void*)my_BringWindowToTop},
        {(void*)real_EnableWindow, (void*)my_EnableWindow},
        {(void*)real_SetWindowLongPtrW, (void*)my_SetWindowLongPtrW},
    };
    for (IMAGE_IMPORT_DESCRIPTOR* imp = (IMAGE_IMPORT_DESCRIPTOR*)(base + dir.VirtualAddress); imp->Name; ++imp) {
        if (lstrcmpiA((const char*)(base + imp->Name), "user32.dll") != 0) continue;
        for (IMAGE_THUNK_DATA* t = (IMAGE_THUNK_DATA*)(base + imp->FirstThunk); t->u1.Function; ++t) {
            for (int i = 0; i < (int)(sizeof map / sizeof map[0]); ++i) {
                if ((void*)t->u1.Function != map[i].real) continue;
                DWORD old;
                if (VirtualProtect(&t->u1.Function, sizeof(void*), PAGE_READWRITE, &old)) {
                    t->u1.Function = (ULONG_PTR)map[i].mine;
                    VirtualProtect(&t->u1.Function, sizeof(void*), old, &old);
                }
            }
        }
    }
}

// Every module but Windows' own and this one: the emulator's exe and, when Qt is a DLL, Qt's.
// Once patched, this DLL must never leave the process: it pins itself.
static void patch_all(void) {
    if (InterlockedExchange(&patched, 1)) return;
    HMODULE pin;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN, (LPCWSTR)&patch_all, &pin);
    HMODULE user32 = GetModuleHandleW(L"user32.dll");
    real_ShowWindow = (void*)GetProcAddress(user32, "ShowWindow");
    real_SetForegroundWindow = (void*)GetProcAddress(user32, "SetForegroundWindow");
    real_SetActiveWindow = (void*)GetProcAddress(user32, "SetActiveWindow");
    real_SetFocus = (void*)GetProcAddress(user32, "SetFocus");
    real_BringWindowToTop = (void*)GetProcAddress(user32, "BringWindowToTop");
    real_EnableWindow = (void*)GetProcAddress(user32, "EnableWindow");
    real_SetWindowLongPtrW = (void*)GetProcAddress(user32, "SetWindowLongPtrW");
    wchar_t sysdir[MAX_PATH];
    const UINT n = GetSystemWindowsDirectoryW(sysdir, MAX_PATH);
    HMODULE mods[1024];
    DWORD bytes = 0;
    if (!EnumProcessModules(GetCurrentProcess(), mods, sizeof mods, &bytes)) return;
    for (DWORD i = 0; i < bytes / sizeof(HMODULE) && i < 1024; ++i) {
        wchar_t path[MAX_PATH];
        if (mods[i] == self || !GetModuleFileNameW(mods[i], path, MAX_PATH)) continue;
        if (n && CompareStringOrdinal(path, (int)n, sysdir, (int)n, TRUE) == CSTR_EQUAL) continue;
        patch_module(mods[i]);
    }
}

// In front of the window's own procedure: it may open and work, but is never shown as active.
static LRESULT CALLBACK quiet_proc(HWND w, UINT msg, WPARAM wp, LPARAM lp) {
    const WNDPROC next = (WNDPROC)GetPropW(w, PROP);
    switch (msg) {
    case WM_WINDOWPOSCHANGING:
        ((WINDOWPOS*)lp)->flags |= SWP_NOACTIVATE;
        break;
    case WM_MOUSEACTIVATE:
        return MA_NOACTIVATE;
    case WM_NCDESTROY:
        SetWindowLongPtrW(w, GWLP_WNDPROC, (LONG_PTR)next);
        RemovePropW(w, PROP);
        break;
    }
    return CallWindowProcW(next, w, msg, wp, lp);
}

// Window event hook, in context: a window of the emulator was just created, before it shows.
__declspec(dllexport) void CALLBACK nx2_event(HWINEVENTHOOK hook, DWORD event, HWND w, LONG obj, LONG child, DWORD thread, DWORD time) {
    (void)hook; (void)thread; (void)time;
    if (event != EVENT_OBJECT_CREATE || !w || obj != OBJID_WINDOW || child != CHILDID_SELF) return;
    if (GetAncestor(w, GA_ROOT) != w || !GetWindow(w, GW_OWNER)) return;  // only owned top-level windows: dialogs, menus
    if (!patched || quiet(w) || !armed()) return;
    const BOOL cloak = TRUE;
    DwmSetWindowAttribute(w, DWMWA_CLOAK, &cloak, sizeof cloak);
    const LONG_PTR ex = GetWindowLongPtrW(w, GWL_EXSTYLE);
    SetWindowLongPtrW(w, GWL_EXSTYLE, (ex | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) & ~WS_EX_APPWINDOW);
    SetPropW(w, PROP, (HANDLE)GetWindowLongPtrW(w, GWLP_WNDPROC));
    SetWindowLongPtrW(w, GWLP_WNDPROC, (LONG_PTR)quiet_proc);
    HWND owner = GetWindow(w, GW_OWNER);
    if (!IsWindowEnabled(owner)) real_EnableWindow(owner, TRUE);
}

// Message hook on the emulator's interface thread: Windows calls it there as soon as a message arrives.
// It patches the emulator once, then tells NX2 Launcher it is ready ("Local\nx2-quiet-ready-<pid>").
__declspec(dllexport) LRESULT CALLBACK nx2_load(int code, WPARAM wp, LPARAM lp) {
    patch_all();
    wchar_t name[64];
    wsprintfW(name, L"Local\\nx2-quiet-ready-%lu", GetCurrentProcessId());
    HANDLE e = OpenEventW(EVENT_MODIFY_STATE, FALSE, name);
    if (e) { SetEvent(e); CloseHandle(e); }
    return CallNextHookEx(NULL, code, wp, lp);
}

// Loading alone does nothing, in NX2 Launcher as in the emulator: the hooks above do the work.
BOOL WINAPI DllMain(HINSTANCE h, DWORD reason, LPVOID reserved) {
    (void)reserved;
    if (reason == DLL_PROCESS_ATTACH) {
        self = h;
        DisableThreadLibraryCalls(h);
    }
    return TRUE;
}
