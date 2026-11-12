// nx2-live.dll: loaded into a running emulator, it applies input profiles at once, as if the player
// had chosen them in the emulator's own Controls window and pressed OK. It only uses Qt's public
// functions, found by name in the emulator's Qt DLLs, so it does not depend on the emulator's build.
//
// Request, next to the DLL (nx2-live.req, UTF-8): one line per player, "<player 0-7>\t<profile name>".
// Result, next to the DLL (nx2-live.res): "ok" or "error: <reason>".
#include <windows.h>

#include <QApplication>
#include <QComboBox>
#include <QDialog>
#include <QFile>
#include <QPointer>
#include <QWidget>

#include <utility>
#include <vector>

static HMODULE g_self;
static HANDLE g_done;
static QString g_base;
static QString g_result;
static std::vector<std::pair<int, QString>> g_jobs;
static QObject* g_watcher;

static void finish(const QString& result) {
    if (g_watcher) {
        qApp->removeEventFilter(g_watcher);
        delete g_watcher;
        g_watcher = nullptr;
    }
    g_result = result;
    SetEvent(g_done);
}

// Runs inside the emulator's Controls window, once it is shown (and kept out of sight).
static void work(QPointer<QDialog> dlg) {
    if (!dlg) return finish("error: the settings window closed too early");
    for (const auto& [player, profile] : g_jobs) {
        auto* tab = dlg->findChild<QWidget*>(QString("tabPlayer%1").arg(player + 1));
        auto* combo = tab ? tab->findChild<QComboBox*>("comboProfiles") : nullptr;
        if (!combo) {
            dlg->reject();
            return finish(QString("error: no profile list for player %1").arg(player + 1));
        }
        const int idx = combo->findText(profile);
        if (idx < 0) {
            dlg->reject();
            return finish(QString("error: profile \"%1\" not found").arg(profile));
        }
        // What the emulator does when the player picks a profile in the list: it loads it.
        combo->setCurrentIndex(idx);
        emit combo->activated(idx);
    }
    // OK: the emulator applies the controls to the running game and saves its settings.
    dlg->accept();
    finish("ok");
}

class Watcher : public QObject {
public:
    bool eventFilter(QObject* o, QEvent* e) override {
        if (!started && e->type() == QEvent::Show && o->isWidgetType()) {
            auto* w = static_cast<QWidget*>(o);
            if (w->isWindow() && qstrcmp(w->metaObject()->className(), "ConfigureDialog") == 0) {
                started = true;
                w->setWindowOpacity(0.0);
                w->move(-32000, -32000);
                QPointer<QDialog> dlg = static_cast<QDialog*>(w);
                QMetaObject::invokeMethod(w, [dlg] { work(dlg); }, Qt::QueuedConnection);
            }
        }
        return false;
    }
    bool started = false;
};

// On the emulator's interface thread.
static void start() {
    if (QApplication::activeModalWidget()) return finish("error: a window of the emulator is open");
    QWidget* main = nullptr;
    for (QWidget* w : QApplication::topLevelWidgets()) {
        const char* c = w->metaObject()->className();
        if (qstrcmp(c, "MainWindow") == 0 || qstrcmp(c, "GMainWindow") == 0) main = w;
    }
    if (!main) return finish("error: main window not found");
    g_watcher = new Watcher;
    qApp->installEventFilter(g_watcher);
    if (!QMetaObject::invokeMethod(main, "OnConfigure", Qt::QueuedConnection)) finish("error: OnConfigure not found");
}

static void write_result() {
    QFile f(g_base + ".res");
    if (f.open(QIODevice::WriteOnly | QIODevice::Truncate)) f.write(g_result.toUtf8());
}

static DWORD WINAPI worker(LPVOID) {
    QFile req(g_base + ".req");
    if (!req.open(QIODevice::ReadOnly)) {
        g_result = "error: no request";
    } else {
        for (const QString& line : QString::fromUtf8(req.readAll()).split('\n', Qt::SkipEmptyParts)) {
            const QStringList parts = line.trimmed().split('\t');
            bool ok = false;
            const int player = parts.value(0).toInt(&ok);
            if (parts.size() == 2 && ok && player >= 0 && player < 8) g_jobs.emplace_back(player, parts[1]);
        }
        if (g_jobs.empty()) g_result = "error: empty request";
    }
    if (!g_jobs.empty()) {
        if (!qApp) {
            g_result = "error: no Qt application";
        } else {
            QMetaObject::invokeMethod(qApp, [] { start(); }, Qt::QueuedConnection);
            if (WaitForSingleObject(g_done, 20000) != WAIT_OBJECT_0) {
                // The watcher may still be in place: the DLL must stay loaded.
                g_result = "error: timed out";
                write_result();
                return 0;
            }
        }
    }
    write_result();
    // Lets the interface thread leave this DLL's code before it goes away.
    Sleep(1500);
    CloseHandle(g_done);
    FreeLibraryAndExitThread(g_self, 0);
}

BOOL WINAPI DllMain(HINSTANCE h, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) {
        g_self = h;
        DisableThreadLibraryCalls(h);
        wchar_t path[MAX_PATH];
        const DWORD n = GetModuleFileNameW(h, path, MAX_PATH);
        g_base = QString::fromWCharArray(path, int(n));
        g_base.chop(4);  // ".dll"
        g_done = CreateEventW(nullptr, TRUE, FALSE, nullptr);
        HANDLE t = CreateThread(nullptr, 0, worker, nullptr, 0, nullptr);
        if (t) CloseHandle(t);
    }
    return TRUE;
}
