#include <windows.h>
#include <agile.h>
#include <cstdlib>
#include <string>

using namespace Windows::ApplicationModel::Activation;
using namespace Windows::ApplicationModel::Core;
using namespace Windows::Foundation;
using namespace Windows::Storage;
using namespace Windows::System::Threading;
using namespace Windows::UI::Core;

int run_probe(void (*report)(const char*, size_t));

namespace {

HANDLE output = INVALID_HANDLE_VALUE;

void write_report(const char* text, size_t length)
{
    DWORD written = 0;
    if (!WriteFile(output, text, static_cast<DWORD>(length), &written, nullptr) || written != length)
        std::abort();
}

void run_to_local_state()
{
    const auto path = std::wstring(ApplicationData::Current->LocalFolder->Path->Data()) +
        L"\\dx11-tier-probe.txt";
    output = CreateFile2(path.c_str(), GENERIC_WRITE, FILE_SHARE_READ, CREATE_ALWAYS, nullptr);
    if (output == INVALID_HANDLE_VALUE) return;
    run_probe(write_report);
    CloseHandle(output);
    output = INVALID_HANDLE_VALUE;
}

ref class ProbeView sealed : IFrameworkView {
    Platform::Agile<CoreWindow> window_;
    bool started_ = false;

    void activate(CoreApplicationView^, IActivatedEventArgs^)
    {
        window_->Activate();
        if (started_) return;
        started_ = true;
        auto dispatcher = window_->Dispatcher;
        ThreadPool::RunAsync(ref new WorkItemHandler([dispatcher](IAsyncAction^) {
            run_to_local_state();
            dispatcher->RunAsync(CoreDispatcherPriority::Normal,
                ref new DispatchedHandler([] { CoreApplication::Exit(); }));
        }));
    }

public:
    virtual void Initialize(CoreApplicationView^ view)
    {
        view->Activated += ref new TypedEventHandler<CoreApplicationView^, IActivatedEventArgs^>(
            this, &ProbeView::activate);
    }

    virtual void SetWindow(CoreWindow^ window) { window_ = window; }
    virtual void Load(Platform::String^) {}
    virtual void Run() { window_->Dispatcher->ProcessEvents(CoreProcessEventsOption::ProcessUntilQuit); }
    virtual void Uninitialize() {}
};

ref class ProbeSource sealed : IFrameworkViewSource {
public:
    virtual IFrameworkView^ CreateView() { return ref new ProbeView(); }
};

}

[Platform::MTAThread]
int main(Platform::Array<Platform::String^>^)
{
    CoreApplication::Run(ref new ProbeSource());
    return 0;
}
