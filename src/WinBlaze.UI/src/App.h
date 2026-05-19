#pragma once

#include "pch.h"
#include "App.xaml.g.h"

namespace winrt::WinBlaze::UI::implementation
{
    struct App : AppT<App>
    {
        App();

        void OnLaunched(Microsoft::UI::Xaml::LaunchActivatedEventArgs const&);

    private:
        Microsoft::UI::Xaml::Window m_window{ nullptr };
    };
}
