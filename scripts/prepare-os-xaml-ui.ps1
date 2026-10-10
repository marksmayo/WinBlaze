#requires -Version 7.0
param([Parameter(Mandatory=$true)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory, $taskRoot)
$taskArtifacts = [IO.Path]::GetFullPath((Join-Path $taskRoot 'artifacts')) + [IO.Path]::DirectorySeparatorChar
if (!$taskOutput.StartsWith($taskArtifacts, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'OS XAML generated source must stay inside repository artifacts'
}
$taskSource = Join-Path $taskRoot 'src/WinBlaze.UI/src'
$taskCandidate = Join-Path $taskOutput 'full-os-candidate'
New-Item -ItemType Directory -Path $taskCandidate -Force | Out-Null
$taskInventory = @()
foreach ($taskFile in Get-ChildItem $taskSource -File | Where-Object Extension -in '.cpp', '.h') {
    $taskContent = Get-Content $taskFile.FullName -Raw
    # Explicit projection changes in an isolated copy. Compiler errors are
    # compatibility evidence; no unsupported behavior may be stubbed out.
    $taskContent = $taskContent.Replace('Microsoft::UI::Xaml::WindowEventArgs', 'Windows::Foundation::IInspectable')
    $taskContent = $taskContent.Replace('Microsoft::UI::Xaml', 'Windows::UI::Xaml')
    $taskContent = $taskContent.Replace('Microsoft::UI::Dispatching', 'Windows::System')
    $taskContent = $taskContent.Replace('Microsoft::UI::Input', 'Windows::UI::Input')
    $taskContent = $taskContent.Replace('Microsoft::UI::Text', 'Windows::UI::Text')
    $taskContent = $taskContent.Replace('winrt/Microsoft.UI.Dispatching.h', 'winrt/Windows.System.h')
    $taskContent = $taskContent.Replace('winrt/Microsoft.UI.Input.h', 'winrt/Windows.UI.Input.h')
    $taskContent = $taskContent.Replace('winrt/Microsoft.UI.Text.h', 'winrt/Windows.UI.Text.h')
    $taskContent = $taskContent.Replace('winrt/Microsoft.UI.Xaml', 'winrt/Windows.UI.Xaml')
    $taskContent = $taskContent.Replace('microsoft.ui.xaml.media.dxinterop.h', 'windows.ui.xaml.media.dxinterop.h')
    if ($taskFile.Name -eq 'MainWindow.cpp') {
        $taskImportWork = @'
        co_await winrt::resume_background();
        uint32_t result = 4;
        try { result = ::WinBlaze::UI::NativeBridge::ImportCsv(path.c_str(), wiztree_utc); } catch (...) {}
        co_await ui_thread;
'@
        $taskImportOwned = @'
        auto operation = BeginOwnedCoroutine();
        if (!operation || IsClosing()) co_return;
        uint32_t result = 4;
        co_await AwaitBackground([&] {
            try { result = ::WinBlaze::UI::NativeBridge::ImportCsv(path.c_str(), wiztree_utc); } catch (...) {}
        });
        if (IsClosing()) co_return;
        co_await ui_thread;
        if (IsClosing()) co_return;
'@
        # Normalize line endings for this exact coroutine anchor.
        $taskContent = $taskContent.Replace("`r`n", "`n")
        $taskImportWork = $taskImportWork.Replace("`r`n", "`n")
        $taskImportOwned = $taskImportOwned.Replace("`r`n", "`n")
        if ([regex]::Matches($taskContent, [regex]::Escape($taskImportWork)).Count -ne 1) {
            throw 'Review changed import background coroutine'
        }
        $taskContent = $taskContent.Replace($taskImportWork, $taskImportOwned)
        $taskExportPattern = '(?s)        co_await winrt::resume_background\(\);\n        uint32_t result = 4;\n        try \{\n(?<body>            result = analysis == 6 .*?)\n        \}\n        catch \(\.\.\.\) \{ result = 4; \}\n        co_await ui_thread;'
        $taskExportRegex = [regex]::new($taskExportPattern)
        if ($taskExportRegex.Matches($taskContent).Count -ne 1) {
            throw 'Review changed catalog export coroutine'
        }
        $taskContent = $taskExportRegex.Replace($taskContent, {
            param($match)
            "        auto operation = BeginOwnedCoroutine();`n" +
            "        if (!operation || IsClosing()) co_return;`n" +
            "        uint32_t result = 4;`n" +
            "        co_await AwaitBackground([&] {`n        try {`n" +
            $match.Groups['body'].Value +
            "`n        } catch (...) { result = 4; }`n        });`n" +
            "        if (IsClosing()) co_return;`n        co_await ui_thread;`n" +
            "        if (IsClosing()) co_return;"
        })
        # The native OS control chrome leaves only a 19px catalog viewport at
        $taskDiscoveryWork = @'
        co_await winrt::resume_background();
        std::vector<::WinBlaze::Portable::Device> devices;
        std::wstring error;
        try { devices = ::WinBlaze::Portable::Devices(); }
        catch (std::exception const& failure) { error = std::wstring(winrt::to_hstring(failure.what())); }
        co_await ui;
'@
        $taskDiscoveryOwned = @'
        std::vector<::WinBlaze::Portable::Device> devices;
        std::wstring error;
        {
            auto operation = BeginOwnedCoroutine();
            if (!operation || IsClosing()) co_return;
            co_await winrt::resume_background();
            try { devices = ::WinBlaze::Portable::Devices(); }
            catch (std::exception const& failure) { error = std::wstring(winrt::to_hstring(failure.what())); }
            if (IsClosing()) co_return;
            co_await ui;
            if (IsClosing()) co_return;
        }
'@
        $taskDiscoveryWork = $taskDiscoveryWork.Replace("`r`n", "`n")
        $taskDiscoveryOwned = $taskDiscoveryOwned.Replace("`r`n", "`n")
        if ([regex]::Matches($taskContent, [regex]::Escape($taskDiscoveryWork)).Count -ne 1) {
            throw 'Review changed portable-device discovery'
        }
        $taskContent = $taskContent.Replace($taskDiscoveryWork, $taskDiscoveryOwned)
        $taskPortableStart = '        auto canceled = std::make_shared<std::atomic_bool>(false);'
        if ([regex]::Matches($taskContent, [regex]::Escape($taskPortableStart)).Count -ne 1) {
            throw 'Review changed portable-device scan admission'
        }
        $taskContent = $taskContent.Replace($taskPortableStart,
            "        auto operation = BeginOwnedCoroutine();`n        if (!operation || IsClosing()) co_return;`n$taskPortableStart")
        $taskPortablePublish = '            if(canceled->load())throw std::runtime_error("Portable-device scan canceled");'
        if ([regex]::Matches($taskContent, [regex]::Escape($taskPortablePublish)).Count -ne 1) {
            throw 'Review changed portable-device publication guard'
        }
        $taskContent = $taskContent.Replace($taskPortablePublish,
            '            if(IsClosing() || canceled->load())throw std::runtime_error("Portable-device scan canceled");')
        $taskPortableCleanup = @'
        if (!snapshot.empty()) { std::error_code ignored; std::filesystem::remove(snapshot, ignored); }
        co_await ui;
        finished->store(true);
'@
        $taskPortableCleanupOwned = @'
        if (!snapshot.empty()) { std::error_code ignored; std::filesystem::remove(snapshot, ignored); }
        if (IsClosing()) co_return;
        co_await ui;
        if (IsClosing()) co_return;
        finished->store(true);
'@
        $taskPortableCleanup = $taskPortableCleanup.Replace("`r`n", "`n")
        $taskPortableCleanupOwned = $taskPortableCleanupOwned.Replace("`r`n", "`n")
        if ([regex]::Matches($taskContent, [regex]::Escape($taskPortableCleanup)).Count -ne 1) {
            throw 'Review changed portable-device snapshot cleanup'
        }
        $taskContent = $taskContent.Replace($taskPortableCleanup, $taskPortableCleanupOwned)
        $taskRootsWork = @'
            winrt::apartment_context ui; co_await winrt::resume_background();
            WbMultiScanResult result{};
            result.status = 4;
            try { result = ::WinBlaze::UI::NativeBridge::ScanRoots(paths.c_str()); } catch (...) {}
            co_await ui;
'@
        $taskRootsOwned = @'
            auto operation = BeginOwnedCoroutine();
            if (!operation || IsClosing()) co_return;
            winrt::apartment_context ui;
            WbMultiScanResult result{};
            result.status = 4;
            co_await AwaitBackground([&] {
                try { result = ::WinBlaze::UI::NativeBridge::ScanRoots(paths.c_str()); } catch (...) {}
            });
            if (IsClosing()) co_return;
            co_await ui;
            if (IsClosing()) co_return;
'@
        $taskRootsWork = $taskRootsWork.Replace("`r`n", "`n")
        $taskRootsOwned = $taskRootsOwned.Replace("`r`n", "`n")
        if ([regex]::Matches($taskContent, [regex]::Escape($taskRootsWork)).Count -ne 1) {
            throw 'Review changed multiple-root background scan'
        }
        $taskContent = $taskContent.Replace($taskRootsWork, $taskRootsOwned)
        $taskCostWork = @'
            winrt::apartment_context ui; co_await winrt::resume_background();
            WbStorageEstimate result{};
            result.status = 4;
            try { result = ::WinBlaze::UI::NativeBridge::StorageCost(policy); }
            catch (...) {}
            co_await ui;
'@
        $taskCostOwned = @'
            WbStorageEstimate result{};
            result.status = 4;
            {
                auto operation = BeginOwnedCoroutine();
                if (!operation || IsClosing()) co_return;
                winrt::apartment_context ui;
                co_await AwaitBackground([&] {
                    try { result = ::WinBlaze::UI::NativeBridge::StorageCost(policy); }
                    catch (...) {}
                });
                if (IsClosing()) co_return;
                co_await ui;
                if (IsClosing()) co_return;
            }
'@
        $taskCostWork = $taskCostWork.Replace("`r`n", "`n")
        $taskCostOwned = $taskCostOwned.Replace("`r`n", "`n")
        if ([regex]::Matches($taskContent, [regex]::Escape($taskCostWork)).Count -ne 1) {
            throw 'Review changed storage-cost background analysis'
        }
        $taskContent = $taskContent.Replace($taskCostWork, $taskCostOwned)
        $taskDuplicatesPattern = '(?s)        winrt::apartment_context ui;\n        co_await winrt::resume_background\(\);\n        WbDuplicateResult result\{\};\n(?<body>        try \{\n            result = ::WinBlaze::UI::NativeBridge::DuplicateResults.*?        \} catch \(\.\.\.\) \{ result.status = 4; \})\n        co_await ui;'
        $taskDuplicatesRegex = [regex]::new($taskDuplicatesPattern)
        if ($taskDuplicatesRegex.Matches($taskContent).Count -ne 1) {
            throw 'Review changed duplicate investigation background analysis'
        }
        $taskContent = $taskDuplicatesRegex.Replace($taskContent, {
            param($match)
            "        WbDuplicateResult result{};`n        {`n" +
            "            auto operation = BeginOwnedCoroutine();`n" +
            "            if (!operation || IsClosing()) co_return;`n" +
            "            winrt::apartment_context ui;`n" +
            "            co_await AwaitBackground([&] {`n" + $match.Groups['body'].Value +
            "`n            });`n            if (IsClosing()) co_return;`n" +
            "            co_await ui;`n            if (IsClosing()) co_return;`n        }"
        })
        $taskPermissionsPattern = '(?s)        winrt::apartment_context ui;\n        co_await winrt::resume_background\(\);\n        uint32_t result = 4;\n(?<body>        try \{\n            result = ::WinBlaze::UI::NativeBridge::PermissionRows.*?        \} catch \(\.\.\.\) \{\})\n        co_await ui;'
        $taskPermissionsRegex = [regex]::new($taskPermissionsPattern)
        if ($taskPermissionsRegex.Matches($taskContent).Count -ne 1) {
            throw 'Review changed permission inspection background work'
        }
        $taskContent = $taskPermissionsRegex.Replace($taskContent, {
            param($match)
            "        uint32_t result = 4;`n        {`n" +
            "            auto operation = BeginOwnedCoroutine();`n" +
            "            if (!operation || IsClosing()) co_return;`n" +
            "            winrt::apartment_context ui;`n" +
            "            co_await AwaitBackground([&] {`n" + $match.Groups['body'].Value +
            "`n            });`n            if (IsClosing()) co_return;`n" +
            "            co_await ui;`n            if (IsClosing()) co_return;`n        }"
        })
        $taskConsolidateWork = @'
        winrt::apartment_context ui;
        co_await winrt::resume_background();
        uint32_t status = 4;
        try { status = ::WinBlaze::UI::NativeBridge::ConsolidateDuplicates(first.c_str(), second.c_str()); }
        catch (...) {}
        co_await ui;
'@
        $taskConsolidateOwned = @'
        auto operation = BeginOwnedCoroutine();
        if (!operation || IsClosing()) co_return;
        winrt::apartment_context ui;
        uint32_t status = 4;
        co_await AwaitBackground([&] {
            try { status = ::WinBlaze::UI::NativeBridge::ConsolidateDuplicates(first.c_str(), second.c_str()); }
            catch (...) {}
        });
        if (IsClosing()) co_return;
        co_await ui;
        if (IsClosing()) co_return;
'@
        $taskInlineWork = @'
        winrt::apartment_context ui;
        uint32_t result=4;
        co_await winrt::resume_background();
        try {result=::WinBlaze::Actions::Rename(path,new_name);}catch(...){}
        co_await ui;
'@
        $taskInlineOwned = @'
        auto operation = BeginOwnedCoroutine();
        if (!operation || IsClosing()) co_return;
        winrt::apartment_context ui;
        uint32_t result=4;
        co_await AwaitBackground([&] {
            try {result=::WinBlaze::Actions::Rename(path,new_name);}catch(...){}
        });
        if (IsClosing()) co_return;
        co_await ui;
        if (IsClosing()) co_return;
'@
        foreach ($taskOperation in @(
            @{Name='consolidation';Before=$taskConsolidateWork;After=$taskConsolidateOwned},
            @{Name='inline rename';Before=$taskInlineWork;After=$taskInlineOwned}
        )) {
            $taskBefore = $taskOperation.Before.Replace("`r`n", "`n")
            $taskAfter = $taskOperation.After.Replace("`r`n", "`n")
            if ([regex]::Matches($taskContent, [regex]::Escape($taskBefore)).Count -ne 1) {
                throw "Review changed $($taskOperation.Name) background work"
            }
            $taskContent = $taskContent.Replace($taskBefore, $taskAfter)
        }
        $taskRenameWork = @'
            winrt::apartment_context ui;
            co_await winrt::resume_background();
            auto result = ::WinBlaze::Actions::Rename(path, new_name);
            co_await ui;
'@
        $taskRenameOwned = @'
            auto operation = BeginOwnedCoroutine();
            if (!operation || IsClosing()) co_return;
            winrt::apartment_context ui;
            decltype(::WinBlaze::Actions::Rename(path, new_name)) result{};
            co_await AwaitBackground([&] {
                result = ::WinBlaze::Actions::Rename(path, new_name);
            });
            if (IsClosing()) co_return;
            co_await ui;
            if (IsClosing()) co_return;
'@
        $taskRenameWork = $taskRenameWork.Replace("`r`n", "`n")
        $taskRenameOwned = $taskRenameOwned.Replace("`r`n", "`n")
        if ([regex]::Matches($taskContent, [regex]::Escape($taskRenameWork)).Count -ne 1) {
            throw 'Review changed dialog rename background work'
        }
        $taskContent = $taskContent.Replace($taskRenameWork, $taskRenameOwned)
        $taskMaintenanceWork = @'
            winrt::apartment_context ui; co_await winrt::resume_background();
            uint32_t result = 4;
            try {
                result = operation == 3 ? ::WinBlaze::VirtualDisks::Compact(path)
                    : ::WinBlaze::UI::NativeBridge::StorageMaintenance(path.c_str(), operation);
            } catch (...) {}

            co_await ui;
'@
        $taskMaintenanceOwned = @'
            auto owned_operation = BeginOwnedCoroutine();
            if (!owned_operation || IsClosing()) co_return;
            winrt::apartment_context ui;
            uint32_t result = 4;
            co_await AwaitBackground([&] {
                try {
                    result = operation == 3 ? ::WinBlaze::VirtualDisks::Compact(path)
                        : ::WinBlaze::UI::NativeBridge::StorageMaintenance(path.c_str(), operation);
                } catch (...) {}
            });
            if (IsClosing()) co_return;
            co_await ui;
            if (IsClosing()) co_return;
'@
        $taskBatchWork = @'
            winrt::apartment_context ui;
            co_await winrt::resume_background();
            auto result = move ? ::WinBlaze::Actions::MoveItems(paths, destination)
                : ::WinBlaze::Actions::RecycleItems(paths);
            co_await ui;
'@
        $taskBatchOwned = @'
            auto operation = BeginOwnedCoroutine();
            if (!operation || IsClosing()) co_return;
            winrt::apartment_context ui;
            DWORD result{};
            co_await AwaitBackground([&] {
                result = move ? ::WinBlaze::Actions::MoveItems(paths, destination)
                    : ::WinBlaze::Actions::RecycleItems(paths);
            });
            if (IsClosing()) co_return;
            co_await ui;
            if (IsClosing()) co_return;
'@
        $taskFileWork = @'
            winrt::apartment_context ui;co_await winrt::resume_background();
            const auto result=move?::WinBlaze::Actions::Move(path,destination) : ::WinBlaze::Actions::Recycle(path);
            co_await ui;m_export_active=false;RefreshScanControls();
'@
        $taskFileOwned = @'
            auto operation = BeginOwnedCoroutine();
            if (!operation || IsClosing()) co_return;
            winrt::apartment_context ui;
            DWORD result{};
            co_await AwaitBackground([&] {
                result=move?::WinBlaze::Actions::Move(path,destination) : ::WinBlaze::Actions::Recycle(path);
            });
            if (IsClosing()) co_return;
            co_await ui;
            if (IsClosing()) co_return;
            m_export_active=false;RefreshScanControls();
'@
        foreach ($taskOperation in @(
            @{Name='maintenance';Before=$taskMaintenanceWork;After=$taskMaintenanceOwned},
            @{Name='batch action';Before=$taskBatchWork;After=$taskBatchOwned},
            @{Name='file action';Before=$taskFileWork;After=$taskFileOwned}
        )) {
            $taskBefore = $taskOperation.Before.Replace("`r`n", "`n")
            $taskAfter = $taskOperation.After.Replace("`r`n", "`n")
            if ([regex]::Matches($taskContent, [regex]::Escape($taskBefore)).Count -ne 1) {
                throw "Review changed $($taskOperation.Name) background work"
            }
            $taskContent = $taskContent.Replace($taskBefore, $taskAfter)
        }
        # Track the complete permissions coroutine, including its user dialogs.
        $taskPermissionStart=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::ExplorePermissions()')
        $taskPermissionEnd=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::ShowChangeHistory()', $taskPermissionStart)
        if($taskPermissionStart -lt 0 -or $taskPermissionEnd -lt 0){throw 'Review permissions coroutine boundaries'}
        $taskPermissions=$taskContent.Substring($taskPermissionStart,$taskPermissionEnd-$taskPermissionStart)
        $taskPermissions=$taskPermissions.Replace('        auto lifetime = get_strong();', "        auto lifetime = get_strong();`n        auto dialog_operation = BeginOwnedCoroutine();`n        if (!dialog_operation || IsClosing()) co_return;")
        if([regex]::Matches($taskPermissions,'\b(options|dialog)\.ShowAsync\(\)').Count -ne 2){throw 'Review permissions dialog sites'}
        $taskPermissions=$taskPermissions.Replace('options.ShowAsync()', 'ShowOwnedDialog(options)').Replace('dialog.ShowAsync()', 'ShowOwnedDialog(dialog)')
        $taskContent=$taskContent.Remove($taskPermissionStart,$taskPermissionEnd-$taskPermissionStart).Insert($taskPermissionStart,$taskPermissions)
        $taskRootsStart=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::ScanMultipleRoots(')
        $taskRootsEnd=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::RunMaintenance()', $taskRootsStart)
        if($taskRootsStart -lt 0 -or $taskRootsEnd -lt 0){throw 'Review multiple-roots coroutine boundaries'}
        $taskRoots=$taskContent.Substring($taskRootsStart,$taskRootsEnd-$taskRootsStart)
        $taskRoots=$taskRoots.Replace('        auto lifetime = get_strong();', "        auto lifetime = get_strong();`n        auto dialog_operation = BeginOwnedCoroutine();`n        if (!dialog_operation || IsClosing()) co_return;")
        if([regex]::Matches($taskRoots,'\bdialog\.ShowAsync\(\)').Count -ne 1){throw 'Review multiple-roots dialog site'}
        $taskRoots=$taskRoots.Replace('dialog.ShowAsync()', 'ShowOwnedDialog(dialog)')
        $taskContent=$taskContent.Remove($taskRootsStart,$taskRootsEnd-$taskRootsStart).Insert($taskRootsStart,$taskRoots)
        $taskMaintenanceStart=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::RunMaintenance()')
        $taskMaintenanceEnd=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::ShowStorageCosts()', $taskMaintenanceStart)
        if($taskMaintenanceStart -lt 0 -or $taskMaintenanceEnd -lt 0){throw 'Review maintenance coroutine boundaries'}
        $taskMaintenanceDialogs=$taskContent.Substring($taskMaintenanceStart,$taskMaintenanceEnd-$taskMaintenanceStart)
        if([regex]::Matches($taskMaintenanceDialogs,[regex]::Escape('        auto lifetime = get_strong();')).Count -ne 1){throw 'Review maintenance owner lifetime'}
        $taskMaintenanceDialogs=$taskMaintenanceDialogs.Replace('        auto lifetime = get_strong();', "        auto lifetime = get_strong();`n        auto dialog_operation = BeginOwnedCoroutine();`n        if (!dialog_operation || IsClosing()) co_return;")
        if([regex]::Matches($taskMaintenanceDialogs,'\b(options|confirm)\.ShowAsync\(\)').Count -ne 2){throw 'Review maintenance dialog sites'}
        $taskMaintenanceDialogs=$taskMaintenanceDialogs.Replace('options.ShowAsync()', 'ShowOwnedDialog(options)').Replace('confirm.ShowAsync()', 'ShowOwnedDialog(confirm)')
        $taskContent=$taskContent.Remove($taskMaintenanceStart,$taskMaintenanceEnd-$taskMaintenanceStart).Insert($taskMaintenanceStart,$taskMaintenanceDialogs)
        $taskHistoryStart=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::ShowChangeHistory()')
        $taskHistoryEnd=$taskContent.IndexOf('    void MainWindow::RestartMonitoring()', $taskHistoryStart)
        if($taskHistoryStart -lt 0 -or $taskHistoryEnd -lt 0){throw 'Review change-history coroutine boundaries'}
        $taskHistory=$taskContent.Substring($taskHistoryStart,$taskHistoryEnd-$taskHistoryStart)
        if([regex]::Matches($taskHistory,[regex]::Escape('        auto lifetime = get_strong();')).Count -ne 1){throw 'Review change-history owner lifetime'}
        $taskHistory=$taskHistory.Replace('        auto lifetime = get_strong();', "        auto lifetime = get_strong();`n        auto dialog_operation = BeginOwnedCoroutine();`n        if (!dialog_operation || IsClosing()) co_return;")
        if([regex]::Matches($taskHistory,'\bdialog\.ShowAsync\(\)').Count -ne 1){throw 'Review change-history dialog site'}
        $taskHistory=$taskHistory.Replace('dialog.ShowAsync()', 'ShowOwnedDialog(dialog)')
        $taskContent=$taskContent.Remove($taskHistoryStart,$taskHistoryEnd-$taskHistoryStart).Insert($taskHistoryStart,$taskHistory)
        $taskImportStart=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::ImportCsv()')
        $taskImportEnd=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::ExportTreemapPng()', $taskImportStart)
        if($taskImportStart -lt 0 -or $taskImportEnd -lt 0){throw 'Review import coroutine boundaries'}
        $taskImportDialogs=$taskContent.Substring($taskImportStart,$taskImportEnd-$taskImportStart)
        if([regex]::Matches($taskImportDialogs,[regex]::Escape('        auto lifetime = get_strong();')).Count -ne 1){throw 'Review import owner lifetime'}
        $taskImportDialogs=$taskImportDialogs.Replace('        auto lifetime = get_strong();', "        auto lifetime = get_strong();`n        auto dialog_operation = BeginOwnedCoroutine();`n        if (!dialog_operation || IsClosing()) co_return;")
        if([regex]::Matches($taskImportDialogs,'\boptions\.ShowAsync\(\)').Count -ne 1){throw 'Review import options dialog site'}
        $taskImportDialogs=$taskImportDialogs.Replace('options.ShowAsync()', 'ShowOwnedDialog(options)')
        $taskContent=$taskContent.Remove($taskImportStart,$taskImportEnd-$taskImportStart).Insert($taskImportStart,$taskImportDialogs)
        $taskPngStart=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::ExportTreemapPng()')
        $taskPngEnd=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::ExportCsv()', $taskPngStart)
        if($taskPngStart -lt 0 -or $taskPngEnd -lt 0){throw 'Review PNG export coroutine boundaries'}
        $taskPng=$taskContent.Substring($taskPngStart,$taskPngEnd-$taskPngStart)
        if([regex]::Matches($taskPng,[regex]::Escape('        auto lifetime = get_strong();')).Count -ne 1){throw 'Review PNG export owner lifetime'}
        $taskPng=$taskPng.Replace('        auto lifetime = get_strong();', "        auto lifetime = get_strong();`n        auto dialog_operation = BeginOwnedCoroutine();`n        if (!dialog_operation || IsClosing()) co_return;")
        if([regex]::Matches($taskPng,'\boptions\.ShowAsync\(\)').Count -ne 1){throw 'Review PNG options dialog site'}
        $taskPng=$taskPng.Replace('options.ShowAsync()', 'ShowOwnedDialog(options)')
        $taskContent=$taskContent.Remove($taskPngStart,$taskPngEnd-$taskPngStart).Insert($taskPngStart,$taskPng)
        $taskCostsStart=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::ShowStorageCosts()')
        $taskCostsEnd=$taskContent.IndexOf('    winrt::fire_and_forget MainWindow::ManageItems()', $taskCostsStart)
        if($taskCostsStart -lt 0 -or $taskCostsEnd -lt 0){throw 'Review storage-cost coroutine boundaries'}
        $taskCosts=$taskContent.Substring($taskCostsStart,$taskCostsEnd-$taskCostsStart)
        if([regex]::Matches($taskCosts,[regex]::Escape('        auto lifetime = get_strong();')).Count -ne 1){throw 'Review storage-cost owner lifetime'}
        $taskCosts=$taskCosts.Replace('        auto lifetime = get_strong();', "        auto lifetime = get_strong();`n        auto dialog_operation = BeginOwnedCoroutine();`n        if (!dialog_operation || IsClosing()) co_return;")
        if([regex]::Matches($taskCosts,'\b(dialog|results)\.ShowAsync\(\)').Count -ne 2){throw 'Review storage-cost dialog sites'}
        $taskCosts=$taskCosts.Replace('dialog.ShowAsync()', 'ShowOwnedDialog(dialog)').Replace('results.ShowAsync()', 'ShowOwnedDialog(results)')
        $taskContent=$taskContent.Remove($taskCostsStart,$taskCostsEnd-$taskCostsStart).Insert($taskCostsStart,$taskCosts)
        # OS XAML templates need additional header space to keep folder expand controls visible.
        $taskTreeFloor = '        shell_r2.MinHeight(180.0);'
        if ([regex]::Matches($taskContent,[regex]::Escape($taskTreeFloor)).Count -ne 1) {
            throw 'Review changed directory card layout floor'
        }
        $taskContent = $taskContent.Replace($taskTreeFloor, '        shell_r2.MinHeight(320.0);')
        # Keep all four producer/cleanup jobs registered with the host. These
        # assertions force review if the shipping source adds another job.
        if ([regex]::Matches($taskContent, 'std::thread\(').Count -ne 4 -or
            [regex]::Matches($taskContent, '\}\)\.detach\(\);').Count -ne 4) {
            throw 'Review changed background producer sites'
        }
        $taskContent = $taskContent.Replace('std::thread(', 'RunBackground(').Replace('}).detach();', '});')
        # Publish handles while callbacks are excluded by their state mutex.
        $taskStartAnchor = '        if (!excluded.empty() || !included.empty()) {'
        $taskStartedAnchor = '        m_session_active = true;'
        if ([regex]::Matches($taskContent, [regex]::Escape($taskStartAnchor)).Count -ne 2 -or
            [regex]::Matches($taskContent, [regex]::Escape($taskStartedAnchor)).Count -ne 1) {
            throw 'Review changed session publication sites'
        }
        # The first identical condition builds rule text; only the second starts
        # the native session, immediately after installing its event handler.
        $taskStartPosition = $taskContent.LastIndexOf($taskStartAnchor)
        $taskContent = $taskContent.Insert($taskStartPosition, "        {`n        std::lock_guard guard(m_pending_ui_mutex);`n        if (IsClosing()) return;`n")
        $taskContent = $taskContent.Replace($taskStartedAnchor, "$taskStartedAnchor`n        }")
        # Both manual cancel routes transfer the handle and register its cleanup
        # before touching UI controls that could reenter the message loop.
        $taskEscapePattern = '(?s)            auto session = m_session;\s*m_session_active = false;\s*m_session = \{\};\s*UpdateStatus\(L"Cancelled\."\);\s*ApplyShellState\(\);\s*RunBackground\(\[session\]\(\) \{\s*::WinBlaze::UI::NativeBridge::CancelScan\(session\);\s*::WinBlaze::UI::NativeBridge::DestroyScan\(session\);\s*\}\);'
        $taskButtonPattern = $taskEscapePattern.Replace('            auto', '        auto')
        $taskCancelRegex = [regex]::new($taskEscapePattern)
        if ($taskCancelRegex.Matches($taskContent).Count -ne 1) { throw 'Review Escape cancellation' }
        $taskContent = $taskCancelRegex.Replace($taskContent, "            CancelOwnedSession();`n            UpdateStatus(L`"Cancelled.`");`n            ApplyShellState();")
        $taskCancelRegex = [regex]::new($taskButtonPattern)
        if ($taskCancelRegex.Matches($taskContent).Count -ne 1) { throw 'Review button cancellation' }
        $taskContent = $taskCancelRegex.Replace($taskContent, "        CancelOwnedSession();`n        UpdateStatus(L`"Cancelled.`");`n        ApplyShellState();")
        $taskCancelHelper = @'
    void MainWindow::CancelOwnedSession()
    {
        std::lock_guard guard(m_pending_ui_mutex);
        auto session = m_session;
        if (!session.native._private) return;
        if (!RunBackground([session]() {
            ::WinBlaze::UI::NativeBridge::CancelScan(session);
            ::WinBlaze::UI::NativeBridge::DestroyScan(session);
        })) winrt::throw_hresult(E_UNEXPECTED);
        m_session = {};
        m_session_active = false;
    }

'@
        $taskHelperAnchor = '    void MainWindow::OnCompositionRendering('
        $taskContent = $taskContent.Replace($taskHelperAnchor, "$taskCancelHelper`n$taskHelperAnchor")
        $taskCloseAnchor = '        if(m_watch_timer)m_watch_timer.Stop();m_watcher.reset();'
        if (-not $taskContent.Contains($taskCloseAnchor)) { throw 'Missing close cleanup anchor' }
        $taskCloseWork = @'
        ++m_snapshot_load_generation;
        {
            std::lock_guard guard(m_pending_ui_mutex);
            auto session = m_session;
            m_session = {};
            m_session_active = false;
            if (session.native._private) {
                RunBackground([session]() {
                    ::WinBlaze::UI::NativeBridge::CancelScan(session);
                    ::WinBlaze::UI::NativeBridge::DestroyScan(session);
                });
            }
        }
'@
        $taskContent = $taskContent.Replace($taskCloseAnchor, "$taskCloseWork`n$taskCloseAnchor")
        $taskEventAnchor = '            // m_scan_started_at / m_last_scan_duration_text / the UI timing'
        if (-not $taskContent.Contains($taskEventAnchor)) { throw 'Missing native event lock anchor' }
        $taskContent = $taskContent.Replace($taskEventAnchor, "            if (IsClosing()) return;`n`n$taskEventAnchor")
        $taskExitCount = [regex]::Matches($taskContent, '(?:winrt::Windows::UI::Xaml::)?Application::Current\(\)\.Exit\(\)').Count
        if ($taskExitCount -ne 2) { throw "Review changed framework exit sites: $taskExitCount" }
        $taskContent = $taskContent.Replace('winrt::Windows::UI::Xaml::Application::Current().Exit()', 'OsDesktopRequestExit()')
        $taskContent = $taskContent.Replace('Application::Current().Exit()', 'OsDesktopRequestExit()')
    }
    if ($taskFile.Name -eq 'MainWindow.xaml.h') {
        $taskContent = $taskContent.Replace('Windows::UI::Xaml::WindowT<MainWindow>', 'OsDesktopWindow<MainWindow>')
        $taskContent = $taskContent.Replace('        MainWindow();', "        MainWindow();`n        void CancelOwnedSession();")
        $taskContent = $taskContent.Replace('bool m_session_active{ false };', 'std::atomic<bool> m_session_active{ false };')
    }
    if ($taskFile.Name -eq 'pch.h') { $taskContent += "`n#include `"../OsDesktopWindow.h`"`n" }
    Set-Content -LiteralPath (Join-Path $taskCandidate $taskFile.Name) -Value $taskContent -NoNewline
    $taskInventory += [pscustomobject]@{file=$taskFile.Name; source_sha256=(Get-FileHash $taskFile.FullName).Hash; candidate_sha256=(Get-FileHash (Join-Path $taskCandidate $taskFile.Name)).Hash}
}
$taskInventory | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $taskOutput 'source-inventory.json')
