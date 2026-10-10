#pragma once
#include <algorithm>
#include <cstdint>
#include <initializer_list>
#include <utility>
#include <vector>
#include <cmath>
#include <limits>
namespace WinBlaze::UI::Treemap {
    struct SpaceBreakdown { uint64_t scanned{}, unaccounted{}, free{}; bool valid{}; };
    inline SpaceBreakdown VolumeSpace(uint64_t total, uint64_t free, uint64_t scanned) {
        if (free > total || scanned > total - free || total == 0) { return {}; }
        return {scanned, total - free - scanned, free, true};
    }
    struct Rect { float left, top, right, bottom; };
    inline bool Visible(Rect r) { return r.right > r.left && r.bottom > r.top; }
    // No minimum pixel size: forcing a 1px split distorts dense directories.
    inline std::pair<Rect,Rect> Split(Rect r, double leading, double total) {
        const float ratio = static_cast<float>(std::clamp(total > 0 ? leading / total : 0.0, 0.0, 1.0));
        if (r.right-r.left >= r.bottom-r.top) {
            const float position = r.left+(r.right-r.left)*ratio;
            return {{r.left,r.top,position,r.bottom},{position,r.top,r.right,r.bottom}};
        }
        const float position = r.top+(r.bottom-r.top)*ratio;
        return {{r.left,r.top,r.right,position},{r.left,position,r.right,r.bottom}};
    }
    inline std::pair<Rect,Rect> SplitHorizontal(Rect r, double leading, double total) {
        const float ratio = static_cast<float>(std::clamp(total > 0 ? leading / total : 0.0, 0.0, 1.0));
        const float position = r.left + (r.right - r.left) * ratio;
        return {{r.left,r.top,position,r.bottom},{position,r.top,r.right,r.bottom}};
    }
    template<class Range> uint64_t RemainingBytes(uint64_t parent, Range const& children) {
        for (uint64_t bytes : children) { parent -= (std::min)(parent,bytes); }
        return parent;
    }
    inline uint64_t RemainingBytes(uint64_t parent, std::initializer_list<uint64_t> children) {
        return RemainingBytes<std::initializer_list<uint64_t>>(parent, children);
    }
    struct HierarchyNode { size_t id; size_t parent; double weight; };
    struct HierarchyTile {
        size_t id{}, depth{};
        Rect bounds{};
        bool radial{};
        double start{}, end{}, inner{}, outer{}, cx{}, cy{};
    };
    // Parents precede their children. Parent indices refer to the input vector.
    // Unrepresented bytes leave a gap; children never exceed their parent's span.
    inline std::vector<HierarchyTile> Hierarchy(std::vector<HierarchyNode> const& nodes, float width, float height, bool radial) {
        if(nodes.empty() || width<=0 || height<=0 || !std::isfinite(width) || !std::isfinite(height))return {};
        std::vector<HierarchyTile> result(nodes.size());
        std::vector<double> cursor(nodes.size());
        size_t max_depth=0;
        constexpr double tau=6.283185307179586;
        for(size_t i=0;i<nodes.size();++i) {
            const auto& node=nodes[i];
            if(!std::isfinite(node.weight) || node.weight<0 || (i>0 && node.parent>=i))return {};
            auto& tile=result[i];tile.id=node.id;tile.radial=radial;
            if(i==0){tile.start=0;tile.end=radial?tau:width;}
            else {
                const auto& parent=result[node.parent];tile.depth=parent.depth+1;
                const double fraction=nodes[node.parent].weight>0?std::clamp(node.weight/nodes[node.parent].weight,0.0,1.0):0;
                tile.start=parent.start+cursor[node.parent];
                tile.end=(std::min)(parent.end,tile.start+(parent.end-parent.start)*fraction);
                cursor[node.parent]=tile.end-parent.start;
            }
            max_depth=(std::max)(max_depth,tile.depth);
        }
        const double radius=(std::min)(width,height)/2.0;
        const double band=(radial?radius:height)/static_cast<double>(max_depth+1);
        for(auto& tile:result) {
            tile.cx=width/2.0;tile.cy=height/2.0;
            tile.inner=band*static_cast<double>(tile.depth);tile.outer=tile.inner+band;
            tile.bounds=radial?Rect{0,0,width,height}:Rect{static_cast<float>(tile.start),static_cast<float>(tile.inner),static_cast<float>(tile.end),static_cast<float>(tile.outer)};
        }
        return result;
    }
    inline bool Contains(HierarchyTile const& tile,double x,double y) {
        if(!tile.radial)return x>=tile.bounds.left&&x<tile.bounds.right&&y>=tile.bounds.top&&y<tile.bounds.bottom;
        const double dx=x-tile.cx,dy=y-tile.cy,r=std::hypot(dx,dy);
        double angle=std::atan2(dy,dx);if(angle<0)angle+=6.283185307179586;
        return r>=tile.inner&&r<tile.outer&&angle>=tile.start&&angle<tile.end;
    }

}
