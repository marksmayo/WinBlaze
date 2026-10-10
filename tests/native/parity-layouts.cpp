#include "../../src/WinBlaze.UI/src/TreemapGeometry.h"
#include "../../src/WinBlaze.UI/src/PermissionView.h"
#include <fstream>
#include <filesystem>
#include <string>
#include <stdexcept>
int wmain(int argc,wchar_t** argv) {
    if(argc!=3)return 2;
    const std::wstring selected=argv[1];
    std::string operation;
    for(wchar_t ch:selected){if(ch>127)return 2;operation.push_back(static_cast<char>(ch));}
    size_t checked=0;
    if(operation=="flame"||operation=="sunburst") {
        using namespace WinBlaze::UI::Treemap;
        std::vector<HierarchyNode> nodes{{0,SIZE_MAX,4096}};
        for(size_t i=1;i<=4096;++i)nodes.push_back({i,0,1});
        for(size_t repeat=0;repeat<100;++repeat) {
            const auto tiles=Hierarchy(nodes,1920,1080,operation=="sunburst");
            if(tiles.size()!=4097)return 1;
            double span=0;
            for(size_t i=1;i<tiles.size();++i)span+=tiles[i].end-tiles[i].start;
            if(std::abs(span-(operation=="sunburst"?6.283185307179586:1920))>0.00001)return 1;
            checked+=tiles.size();
        }
    } else if(operation=="permissions") {
        using namespace WinBlaze::UI::Permissions;
        const Policy policy{L"bob",{{L"alice",0x120089,1}}};
        for(size_t repeat=0;repeat<100;++repeat) {
            size_t visible=0,highlighted=0;
            for(size_t i=0;i<4096;++i) {
                Row row{L"Allow",i%2?L"DOMAIN\\Alice":L"DOMAIN\\Bob",L"S-1-5-21",L"Read",false,0x120089};
                const auto match=policy.Apply(row,L"read");
                if(match.visible)++visible;if(match.visible&&match.highlighted)++highlighted;
            }
            if(visible!=2048||highlighted!=2048)return 1;
            checked+=4096;
        }
    } else return 2;
    std::ofstream output(std::filesystem::path(argv[2]),std::ios::binary);
    output<<"{\"operation\":\""<<operation<<"\",\"verified\":true,\"records_checked\":"<<checked<<",\"repeats\":100}\n";
    return output?0:1;
}
