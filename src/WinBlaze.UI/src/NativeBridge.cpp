#include "pch.h"
#include "NativeBridge.h"

#include <stdexcept>
#include <filesystem>
#include <memory>
#include <string>

namespace
{
    using StartScanFn = WbScanSessionHandle(__stdcall *)(WbCStringView, WbEventCallback, void*);
    using CancelScanFn = void(__stdcall *)(WbScanSessionHandle);
    using DestroyScanFn = void(__stdcall *)(WbScanSessionHandle);
    using LoadCatalogSnapshotWithStatsFn = WbIndexSnapshotStats(__stdcall *)(WbCatalogCallback, void*);
    using LoadExtensionStatsFn = void(__stdcall *)(WbExtensionStatCallback, void*);
    using TreeRootFn = uint8_t(__stdcall *)(WbTreeNodeCallback, void*);
    using TreeChildrenFn = WbTreeChildrenResult(__stdcall *)(uint64_t, uint64_t, WbTreeNodeCallback, void*);
    using TreeLargestFilesFn = void(__stdcall *)(uint64_t, WbTreeNodeCallback, void*);

    struct DllApi
    {
        HMODULE module{ nullptr };
        StartScanFn start_scan{ nullptr };
        StartScanFn start_incremental_scan{ nullptr };
        CancelScanFn cancel_scan{ nullptr };
        DestroyScanFn destroy_scan{ nullptr };
        LoadCatalogSnapshotWithStatsFn load_catalog_snapshot_with_stats{ nullptr };
        LoadExtensionStatsFn load_extension_stats{ nullptr };
        TreeRootFn tree_root{ nullptr };
        TreeChildrenFn tree_children{ nullptr };
        TreeLargestFilesFn tree_largest_files{ nullptr };
    };

    DllApi& Api()
    {
        static DllApi api;
        return api;
    }

    std::filesystem::path ExeDirectory()
    {
        std::wstring buffer(MAX_PATH, L'\0');
        const auto length = GetModuleFileNameW(nullptr, buffer.data(), static_cast<DWORD>(buffer.size()));
        if (length == 0) {
            return {};
        }

        buffer.resize(length);
        return std::filesystem::path(buffer).parent_path();
    }

    struct CallbackContext
    {
        WinBlaze::UI::NativeBridge::EventHandler handler;
    };

    struct CatalogCallbackContext
    {
        WinBlaze::UI::NativeBridge::CatalogHandler handler;
    };

    struct ExtensionStatCallbackContext
    {
        WinBlaze::UI::NativeBridge::ExtensionStatHandler handler;
    };

    struct TreeNodeCallbackContext
    {
        WinBlaze::UI::NativeBridge::TreeNodeHandler handler;
    };

    void EnsureLoaded()
    {
        auto& api = Api();
        if (api.module != nullptr) {
            return;
        }

        const auto dll_path = ExeDirectory() / L"winblaze_native.dll";
        api.module = LoadLibraryW(dll_path.c_str());
        if (api.module == nullptr) {
            throw std::runtime_error("Failed to load winblaze_native.dll");
        }

        api.start_scan = reinterpret_cast<StartScanFn>(GetProcAddress(api.module, "wb_scan_session_start"));
        api.start_incremental_scan = reinterpret_cast<StartScanFn>(GetProcAddress(api.module, "wb_scan_session_start_incremental"));
        api.cancel_scan = reinterpret_cast<CancelScanFn>(GetProcAddress(api.module, "wb_scan_session_cancel"));
        api.destroy_scan = reinterpret_cast<DestroyScanFn>(GetProcAddress(api.module, "wb_scan_session_destroy"));
        api.load_catalog_snapshot_with_stats = reinterpret_cast<LoadCatalogSnapshotWithStatsFn>(GetProcAddress(api.module, "wb_index_snapshot_load_with_stats"));
        api.load_extension_stats = reinterpret_cast<LoadExtensionStatsFn>(GetProcAddress(api.module, "wb_index_snapshot_extension_stats"));
        api.tree_root = reinterpret_cast<TreeRootFn>(GetProcAddress(api.module, "wb_tree_root"));
        api.tree_children = reinterpret_cast<TreeChildrenFn>(GetProcAddress(api.module, "wb_tree_children"));
        api.tree_largest_files = reinterpret_cast<TreeLargestFilesFn>(GetProcAddress(api.module, "wb_tree_largest_files"));

        if (api.start_scan == nullptr || api.start_incremental_scan == nullptr || api.cancel_scan == nullptr || api.destroy_scan == nullptr || api.load_catalog_snapshot_with_stats == nullptr || api.load_extension_stats == nullptr || api.tree_root == nullptr || api.tree_children == nullptr || api.tree_largest_files == nullptr) {
            throw std::runtime_error("Failed to resolve winblaze_native exports");
        }
    }

    extern "C" void __stdcall OnEventWithContext(const WbEvent* event, void* user_data)
    {
        if (event == nullptr || user_data == nullptr) {
            return;
        }

        auto* context = static_cast<CallbackContext*>(user_data);
        if (context->handler) {
            context->handler(*event);
        }
    }

    extern "C" void __stdcall OnCatalogWithContext(const WbCatalogEntry* entry, void* user_data)
    {
        if (entry == nullptr || user_data == nullptr) {
            return;
        }

        auto* context = static_cast<CatalogCallbackContext*>(user_data);
        if (context->handler) {
            context->handler(*entry);
        }
    }

    extern "C" void __stdcall OnExtensionStatWithContext(const WbExtensionStat* entry, void* user_data)
    {
        if (entry == nullptr || user_data == nullptr) {
            return;
        }

        auto* context = static_cast<ExtensionStatCallbackContext*>(user_data);
        if (context->handler) {
            context->handler(*entry);
        }
    }

    extern "C" void __stdcall OnTreeNodeWithContext(const WbTreeNode* node, void* user_data)
    {
        if (node == nullptr || user_data == nullptr) {
            return;
        }

        auto* context = static_cast<TreeNodeCallbackContext*>(user_data);
        if (context->handler) {
            context->handler(*node);
        }
    }

    std::string ToUtf8(const wchar_t* input)
    {
        if (input == nullptr) {
            return {};
        }

        const int required = WideCharToMultiByte(CP_UTF8, 0, input, -1, nullptr, 0, nullptr, nullptr);
        if (required <= 0) {
            return {};
        }

        std::string output(static_cast<size_t>(required), '\0');
        WideCharToMultiByte(CP_UTF8, 0, input, -1, output.data(), required, nullptr, nullptr);
        output.resize(static_cast<size_t>(required - 1));
        return output;
    }
}

namespace WinBlaze::UI::NativeBridge
{
    void Initialize()
    {
        EnsureLoaded();
    }

    SessionHandle StartScanWith(
        StartScanFn start_scan,
        const wchar_t* rootPath,
        EventHandler handler)
    {
        EnsureLoaded();
        const std::string utf8 = ToUtf8(rootPath);
        auto view = WbCStringView{ utf8.data(), utf8.size() };
        auto context = std::make_shared<CallbackContext>();
        context->handler = std::move(handler);

        SessionHandle session;
        session.callback_state = context;
        session.native = start_scan(view, &OnEventWithContext, context.get());
        return session;
    }

    SessionHandle StartScan(const wchar_t* rootPath, EventHandler handler)
    {
        EnsureLoaded();
        return StartScanWith(Api().start_scan, rootPath, std::move(handler));
    }

    SessionHandle StartIncrementalScan(const wchar_t* rootPath, EventHandler handler)
    {
        EnsureLoaded();
        return StartScanWith(Api().start_incremental_scan, rootPath, std::move(handler));
    }

    void CancelScan(SessionHandle handle)
    {
        EnsureLoaded();
        Api().cancel_scan(handle.native);
    }

    void DestroyScan(SessionHandle handle)
    {
        EnsureLoaded();
        Api().destroy_scan(handle.native);
    }

    WbIndexSnapshotStats LoadCatalogSnapshotWithStats(CatalogHandler handler)
    {
        EnsureLoaded();
        auto context = std::make_shared<CatalogCallbackContext>();
        context->handler = std::move(handler);
        return Api().load_catalog_snapshot_with_stats(&OnCatalogWithContext, context.get());
    }

    void LoadExtensionStatsSnapshot(ExtensionStatHandler handler)
    {
        EnsureLoaded();
        auto context = std::make_shared<ExtensionStatCallbackContext>();
        context->handler = std::move(handler);
        Api().load_extension_stats(&OnExtensionStatWithContext, context.get());
    }

    bool TreeRoot(TreeNodeHandler handler)
    {
        EnsureLoaded();
        auto context = std::make_shared<TreeNodeCallbackContext>();
        context->handler = std::move(handler);
        return Api().tree_root(&OnTreeNodeWithContext, context.get()) != 0;
    }

    WbTreeChildrenResult TreeChildren(uint64_t parentId, uint64_t offset, TreeNodeHandler handler)
    {
        EnsureLoaded();
        auto context = std::make_shared<TreeNodeCallbackContext>();
        context->handler = std::move(handler);
        return Api().tree_children(parentId, offset, &OnTreeNodeWithContext, context.get());
    }

    void TreeLargestFiles(uint64_t limit, TreeNodeHandler handler)
    {
        EnsureLoaded();
        auto context = std::make_shared<TreeNodeCallbackContext>();
        context->handler = std::move(handler);
        Api().tree_largest_files(limit, &OnTreeNodeWithContext, context.get());
    }
}
