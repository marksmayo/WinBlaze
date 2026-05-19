#pragma once

#include "XamlMetaDataProvider.g.h"

namespace winrt::WinBlaze::UI::implementation
{
    struct XamlMetaDataProvider : XamlMetaDataProviderT<XamlMetaDataProvider>
    {
        XamlMetaDataProvider() = default;

        winrt::Microsoft::UI::Xaml::Markup::IXamlType GetXamlType(
            winrt::Windows::UI::Xaml::Interop::TypeName const& type);
        winrt::Microsoft::UI::Xaml::Markup::IXamlType GetXamlType(winrt::hstring const& fullName);
        winrt::com_array<winrt::Microsoft::UI::Xaml::Markup::XmlnsDefinition> GetXmlnsDefinitions();
    };
}

namespace winrt::WinBlaze::UI::factory_implementation
{
    struct XamlMetaDataProvider :
        XamlMetaDataProviderT<XamlMetaDataProvider, implementation::XamlMetaDataProvider>
    {
    };
}
