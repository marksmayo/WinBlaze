#include "pch.h"
#include "App.h"
#include "MainWindow.h"
#include "StartupTrace.h"

#include <winrt/Microsoft.UI.Windowing.h>
#include <winrt/Windows.Graphics.h>
#include <microsoft.ui.xaml.window.h>

using namespace winrt;
using namespace Microsoft::UI::Xaml;

namespace winrt::WinBlaze::UI::implementation
{
    App::App()
    {
        TraceStartup(L"App::App begin");
        try {
            InitializeComponent();
            TraceStartup(L"App::App after InitializeComponent");
        }
        catch (winrt::hresult_error const& error) {
            std::wstring message = L"App construction failed: ";
            message += error.message().c_str();
            TraceStartup(message);
            ReportFailure(L"App construction", message);
            ::MessageBoxW(nullptr, message.c_str(), L"WinBlaze startup error", MB_OK | MB_ICONERROR);
            throw;
        }
        catch (std::exception const& error) {
            std::wstring message = L"App construction failed: ";
            message += winrt::to_hstring(error.what()).c_str();
            TraceStartup(message);
            ReportFailure(L"App construction", message);
            ::MessageBoxW(nullptr, message.c_str(), L"WinBlaze startup error", MB_OK | MB_ICONERROR);
            throw;
        }
    }

    void App::OnLaunched(LaunchActivatedEventArgs const&)
    {
        TraceStartup(L"App::OnLaunched begin");
        try {
            TraceStartup(L"App::OnLaunched before MainWindow");
            m_window = winrt::make<MainWindow>().as<Microsoft::UI::Xaml::Window>();
            m_window.Title(L"WinBlaze");
            TraceStartup(L"App::OnLaunched after MainWindow");
            m_window.Activate();
            TraceStartup(L"App::OnLaunched after Activate");
            auto app_window = m_window.AppWindow();
            app_window.Title(L"WinBlaze");
            if (auto presenter = app_window.Presenter().try_as<Microsoft::UI::Windowing::OverlappedPresenter>()) {
                presenter.IsResizable(true);
                presenter.IsMaximizable(true);
                presenter.IsMinimizable(true);
                presenter.Maximize();
                TraceStartup(L"App::OnLaunched configured overlapped presenter");
            }
            app_window.Show();
            TraceStartup(L"App::OnLaunched after AppWindow.Show");
            if (auto window_native = m_window.as<IWindowNative>()) {
                HWND hwnd{};
                if (SUCCEEDED(window_native->get_WindowHandle(&hwnd)) && hwnd) {
                    ::SetWindowTextW(hwnd, L"WinBlaze");
                    ::ShowWindow(hwnd, SW_MAXIMIZE);
                    ::SetForegroundWindow(hwnd);
                    TraceStartup(L"App::OnLaunched forced HWND maximized");
                } else {
                    TraceStartup(L"App::OnLaunched no HWND from IWindowNative");
                }
            }
        }
        catch (winrt::hresult_error const& error) {
            std::wstring message = L"App::OnLaunched failed: ";
            message += error.message().c_str();
            TraceStartup(message);
            ReportFailure(L"App::OnLaunched", message);
            ::MessageBoxW(nullptr, message.c_str(), L"WinBlaze startup error", MB_OK | MB_ICONERROR);
            throw;
        }
        catch (std::exception const& error) {
            std::wstring message = L"App::OnLaunched failed: ";
            message += winrt::to_hstring(error.what()).c_str();
            TraceStartup(message);
            ReportFailure(L"App::OnLaunched", message);
            ::MessageBoxW(nullptr, message.c_str(), L"WinBlaze startup error", MB_OK | MB_ICONERROR);
            throw;
        }
    }
}
