// nx2-inject <pid> <dll>: loads the DLL into the process, then prints the result the DLL leaves next to itself.
#include <windows.h>

#include <cstdio>
#include <string>

int wmain(int argc, wchar_t** argv) {
    if (argc != 3) { fwprintf(stderr, L"usage: nx2-inject <pid> <dll>\n"); return 2; }
    const DWORD pid = wcstoul(argv[1], nullptr, 10);
    wchar_t dll[MAX_PATH];
    if (!GetFullPathNameW(argv[2], MAX_PATH, dll, nullptr)) return 2;
    std::wstring res(dll);
    res = res.substr(0, res.size() - 4) + L".res";
    DeleteFileW(res.c_str());

    HANDLE p = OpenProcess(PROCESS_CREATE_THREAD | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);
    if (!p) { wprintf(L"error: cannot open process %lu (%lu)\n", pid, GetLastError()); return 1; }
    const SIZE_T bytes = (wcslen(dll) + 1) * sizeof(wchar_t);
    void* mem = VirtualAllocEx(p, nullptr, bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if (!mem || !WriteProcessMemory(p, mem, dll, bytes, nullptr)) { wprintf(L"error: cannot write the path\n"); return 1; }
    auto load = reinterpret_cast<LPTHREAD_START_ROUTINE>(GetProcAddress(GetModuleHandleW(L"kernel32.dll"), "LoadLibraryW"));
    HANDLE t = CreateRemoteThread(p, nullptr, 0, load, mem, 0, nullptr);
    if (!t) { wprintf(L"error: cannot start the loader (%lu)\n", GetLastError()); return 1; }
    WaitForSingleObject(t, 10000);
    DWORD loaded = 0;
    GetExitCodeThread(t, &loaded);
    CloseHandle(t);
    VirtualFreeEx(p, mem, 0, MEM_RELEASE);
    CloseHandle(p);
    if (!loaded) { wprintf(L"error: the DLL did not load\n"); return 1; }

    for (int i = 0; i < 250; ++i) {
        HANDLE f = CreateFileW(res.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING, 0, nullptr);
        if (f != INVALID_HANDLE_VALUE) {
            char buf[512] = {};
            DWORD n = 0;
            ReadFile(f, buf, sizeof buf - 1, &n, nullptr);
            CloseHandle(f);
            if (n) { printf("%s\n", buf); return strncmp(buf, "ok", 2) == 0 ? 0 : 1; }
        }
        Sleep(100);
    }
    wprintf(L"error: no answer\n");
    return 1;
}
