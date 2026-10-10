#include "../../src/WinBlaze.UI/src/PermissionView.h"
#include <stdexcept>
#include <iostream>
int main() {
    using namespace WinBlaze::UI::Permissions;
    const Row alice{L"Allow",L"DOMAIN\\Alice",L"S-1-1",L"Read",false};
    const Row bob{L"Deny",L"DOMAIN\\Bob",L"S-1-2",L"Write",true};
    if(!Evaluate(alice,L"read",L"",L"alice").visible || !Evaluate(alice,L"",L"",L"alice").highlighted)
        throw std::runtime_error("Matching account must highlight independently of row filter");
    if(Evaluate(alice,L"",L"bob; ALICE",L"alice").visible || Evaluate(bob,L"read",L"",L"").visible)
        throw std::runtime_error("Excluded accounts and nonmatching rows must be hidden");
    if(Evaluate(bob,L"",L"",L"read").highlighted || !Evaluate(bob,L"write",L"",L"S-1-2").highlighted)
        throw std::runtime_error("Highlight matches account or SID, never incidental rights");
    const Rule regex_rule{L"domain\\\\(alice|bob)$",0,2};
    const Policy policy{L"^S-1-2$",{regex_rule}};
    if(!policy.Valid() || !policy.Apply(alice,L"").highlighted || policy.Apply(bob,L"").visible)
        throw std::runtime_error("Regex accounts must highlight and exclude by SID");
    const Policy invalid{L"[",{}};
    if(invalid.Valid())throw std::runtime_error("Malformed exclusion regex must be reported");
    const Row reader{L"Allow",L"Alice",L"S-1-3",L"Read",false,0x120089};
    if(Policy{L"",{{L"Alice",0x120116,1}}}.Apply(reader,L"").highlighted || !Policy{L"",{{L"Alice",0x120089,1}}}.Apply(reader,L"").highlighted)
        throw std::runtime_error("Highlight rights thresholds must inspect ACE masks");
    std::cout << "PASS: account exclusions, SID highlights and rights filtering\n";
}
