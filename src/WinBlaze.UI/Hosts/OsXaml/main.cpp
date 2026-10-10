#include "full-os-candidate/MainWindow.xaml.h"
#include <exception>
#include "full-os-candidate/StartupTrace.h"
#include "DrainBeforeRelease.h"

// Balance only this additional COM initialization after the window and XAML manager release.
// WinRT initialization and its existing apartment policy remain unchanged.
class ScopedNoLegacyDde final {
public:
    ScopedNoLegacyDde() {
        winrt::check_hresult(CoInitializeEx(nullptr,
            COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE));
    }
    ~ScopedNoLegacyDde() { CoUninitialize(); }
    ScopedNoLegacyDde(ScopedNoLegacyDde const&) = delete;
    ScopedNoLegacyDde& operator=(ScopedNoLegacyDde const&) = delete;
};
// Entry point for the OS XAML Release desktop host.
int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR, int)
{
    SetUnhandledExceptionFilter(WinBlazeUnhandledExceptionFilter);
    TraceStartup(L"wWinMain begin");
    try {
        ScopedNoLegacyDde no_legacy_dde;
        winrt::init_apartment(winrt::apartment_type::single_threaded);
        // Per-monitor-v2 awareness is established by the embedded manifest.
        auto manager = winrt::Windows::UI::Xaml::Hosting::WindowsXamlManager::InitializeForCurrentThread();
        auto window = winrt::make_self<winrt::WinBlaze::UI::implementation::MainWindow>();
        auto weak_window = window->get_weak();
        DrainBeforeRelease shutdown(window, [&manager] {
            manager.Close();
            MSG pending{};
            while (PeekMessageW(&pending, nullptr, 0, 0, PM_REMOVE)) {
                TranslateMessage(&pending);
                DispatchMessageW(&pending);
            }
        });
        HWND owner{};
        winrt::check_hresult(window.as<IWindowNative>()->get_WindowHandle(&owner));
        window->Title(L"WinBlaze");
        window->Activate();
        ShowWindow(owner, SW_MAXIMIZE);
        MSG message{};
        while (IsWindow(owner) && GetMessageW(&message, nullptr, 0, 0) > 0) {
            if (!window->PreTranslate(message)) {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        shutdown.Finish();
        window = nullptr;
        if (weak_window.get()) {
            ReportFailure(L"wWinMain", L"Window remains alive after completed shutdown");
            return 97;
        }
        return 0;
    } catch (winrt::hresult_error const& error) {
        std::wstring message = L"wWinMain failed: ";
        message += error.message().c_str();
        TraceStartup(message);
        ReportFailure(L"wWinMain", message);
        MessageBoxW(nullptr, message.c_str(), L"WinBlaze startup error", MB_OK | MB_ICONERROR);
        return static_cast<int>(error.code().value);
    } catch (std::exception const& error) {
        std::wstring message = L"wWinMain failed: ";
        message += winrt::to_hstring(error.what()).c_str();
        TraceStartup(message);
        ReportFailure(L"wWinMain", message);
        MessageBoxW(nullptr, message.c_str(), L"WinBlaze startup error", MB_OK | MB_ICONERROR);
        return 3;
    } catch (...) {
        ReportFailure(L"wWinMain", L"Unknown startup failure");
        MessageBoxW(nullptr, L"An unknown startup failure occurred.", L"WinBlaze startup error", MB_OK | MB_ICONERROR);
        return 98;
    }
}
