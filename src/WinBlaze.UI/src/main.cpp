#include "pch.h"
#include "App.h"
#include "StartupTrace.h"

#include <MddBootstrap.h>
#include <WindowsAppSDK-VersionInfo.h>

int __stdcall wWinMain(
    HINSTANCE,
    HINSTANCE,
    PWSTR,
    int)
{
    TraceStartup(L"wWinMain begin");
    SetUnhandledExceptionFilter(WinBlazeUnhandledExceptionFilter);
    try {
        PACKAGE_VERSION min_version{};
        min_version.Version = WINDOWSAPPSDK_RUNTIME_VERSION_UINT64;
        const HRESULT bootstrap_hr = MddBootstrapInitialize2(
            WINDOWSAPPSDK_RELEASE_MAJORMINOR,
            WINDOWSAPPSDK_RELEASE_VERSION_TAG_W,
            min_version,
            MddBootstrapInitializeOptions_None);
        if (FAILED(bootstrap_hr)) {
            TraceStartup(L"wWinMain bootstrap failed");
            ReportFailure(L"bootstrap", L"Windows App SDK bootstrap failed");
            return static_cast<int>(bootstrap_hr);
        }
        TraceStartup(L"wWinMain after bootstrap");
        winrt::init_apartment(winrt::apartment_type::single_threaded);
        TraceStartup(L"wWinMain after init_apartment");
        winrt::Microsoft::UI::Xaml::Application::Start([](auto&&) {
            TraceStartup(L"Application::Start callback begin");
            static winrt::com_ptr<winrt::WinBlaze::UI::implementation::App> app;
            app = winrt::make_self<winrt::WinBlaze::UI::implementation::App>();
            TraceStartup(L"Application::Start callback after App");
        });
        TraceStartup(L"wWinMain after Application::Start");
        return 0;
    }
    catch (winrt::hresult_error const& error) {
        std::wstring message = L"wWinMain failed: ";
        message += error.message().c_str();
        ::MessageBoxW(nullptr, message.c_str(), L"WinBlaze startup error", MB_OK | MB_ICONERROR);
        TraceStartup(message);
        ReportFailure(L"wWinMain", message);
        return static_cast<int>(error.code());
    }
    catch (std::exception const& error) {
        std::wstring message = L"wWinMain failed: ";
        message += winrt::to_hstring(error.what()).c_str();
        ::MessageBoxW(nullptr, message.c_str(), L"WinBlaze startup error", MB_OK | MB_ICONERROR);
        TraceStartup(message);
        ReportFailure(L"wWinMain", message);
        return 3;
    }
}
