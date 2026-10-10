#include "../../src/WinBlaze.UI/src/ColorResourceCache.h"
#include <memory>
#include <stdexcept>
#include <iostream>
int main() {
    WinBlaze::ColorResourceCache<std::shared_ptr<int>, 2> cache;
    int created=0;
    auto create=[&](uint32_t color){++created;return std::make_shared<int>(color);};
    auto first=cache.Get(0xff123456,create);
    for(int i=0;i<100;++i)if(cache.Get(0xff123456,create)!=first)return 1;
    if(created!=1||*first!=static_cast<int>(0xff123456))return 2;
    auto other=cache.Get(0x80123456,create);
    if(other==first||created!=2)return 3;
    cache.Get(0xffffffff,create);cache.Get(0xffffffff,create);
    if(created!=4||cache.Get(0xff123456,create)!=first)return 4;
    bool refused=false;
    try{cache.Get(0x00000000,[](uint32_t)->std::shared_ptr<int>{throw std::runtime_error("factory");});}catch(std::runtime_error const&){refused=true;}
    if(!refused||cache.Get(0xff123456,create)!=first)return 5;
    std::cout<<"PASS: repeated colors share resources; alpha is distinct, retention bounded, failures preserve existing resources\n";
}
