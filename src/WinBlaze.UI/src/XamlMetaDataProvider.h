#pragma once

#include "pch.h"

#include <winrt/Microsoft.UI.Xaml.Markup.h>

namespace winrt::WinBlaze::UI::implementation
{
    struct XamlMetaDataProvider :
        winrt::implements<XamlMetaDataProvider, winrt::Microsoft::UI::Xaml::Markup::IXamlMetadataProvider>
    {
        XamlMetaDataProvider() = default;

        winrt::Microsoft::UI::Xaml::Markup::IXamlType GetXamlType(
            winrt::Windows::UI::Xaml::Interop::TypeName const& type);
        winrt::Microsoft::UI::Xaml::Markup::IXamlType GetXamlType(winrt::hstring const& fullName);
        winrt::com_array<winrt::Microsoft::UI::Xaml::Markup::XmlnsDefinition> GetXmlnsDefinitions();
    };
}
