#pragma once
#include <string>
#include <algorithm>
#include <cwctype>
#include <regex>
#include <vector>
#include <optional>
#include <cstdint>
namespace WinBlaze::UI::Permissions {
    struct Row { std::wstring kind, account, sid, rights; bool inherited{}; uint32_t mask{}; };
    struct Match { bool visible{}, highlighted{}; size_t color{}; };
    inline std::wstring Fold(std::wstring value) {
        std::transform(value.begin(),value.end(),value.begin(),[](wchar_t c){return static_cast<wchar_t>(std::towlower(c));});
        return value;
    }
    inline bool Accounts(Row const& row,std::wstring const& patterns) {
        const auto account=Fold(row.account),sid=Fold(row.sid);
        size_t start=0;
        while(start<patterns.size()) {
            const auto end=patterns.find_first_of(L";\r\n",start);
            auto token=patterns.substr(start,end==std::wstring::npos?end:end-start);
            const auto first=token.find_first_not_of(L" \t");
            if(first!=std::wstring::npos) {
                token=Fold(token.substr(first,token.find_last_not_of(L" \t")-first+1));
                if(account.find(token)!=std::wstring::npos||sid.find(token)!=std::wstring::npos)return true;
            }
            if(end==std::wstring::npos)break;
            start=end+1;
        }
        return false;
    }
    inline Match Evaluate(Row const& row,std::wstring const& filter,std::wstring const& excluded,std::wstring const& highlighted) {
        const auto text=Fold(row.kind+L" | "+row.account+L" | "+row.sid+L" | "+row.rights+(row.inherited?L" | Inherited":L" | Explicit"));
        return {(filter.empty()||text.find(Fold(filter))!=std::wstring::npos)&&!Accounts(row,excluded),Accounts(row,highlighted)};
    }
    struct Rule { std::wstring account_regex; uint32_t minimum_mask{}; size_t color{}; };
    // Compile once per settings change, never once per ACL row.
    class Policy {
        struct Compiled { std::wregex account; uint32_t mask{}; size_t color{}; };
        std::optional<std::wregex> m_excluded;
        std::vector<Compiled> m_rules;
        bool m_valid{true};
    public:
        Policy(std::wstring const& excluded,std::vector<Rule> const& rules) {
            try {
                const auto flags=std::regex_constants::ECMAScript|std::regex_constants::icase|std::regex_constants::optimize;
                if(!excluded.empty())m_excluded.emplace(excluded,flags);
                for(const auto& rule:rules)if(!rule.account_regex.empty())m_rules.push_back({std::wregex(rule.account_regex,flags),rule.minimum_mask,rule.color});
            }catch(const std::regex_error&){m_valid=false;}
        }
        bool Valid() const {return m_valid;}
        Match Apply(Row const& row,std::wstring const& filter) const {
            auto result=Evaluate(row,filter,L"",L"");
            if(!m_valid)return result;
            auto matches=[&](std::wregex const& regex){return std::regex_search(row.account,regex)||std::regex_search(row.sid,regex);};
            if(m_excluded&&matches(*m_excluded))result.visible=false;
            for(const auto& rule:m_rules)if(matches(rule.account)&&(row.mask&rule.mask)==rule.mask) {
                result.highlighted=true;result.color=rule.color;break;
            }
            return result;
        }
    };

}
