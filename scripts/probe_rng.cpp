// Probe the consumer chain: does the loose test re-evaluate when the fanout changes?
#include "Vtb_fi_entropy_src.h"
#include "Vtb_fi_entropy_src___024root.h"
#include "Vtb_fi_entropy_src_entropy_src.h"
#include "verilated.h"
#include <cstdio>
#include <cstring>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    auto* tb = new Vtb_fi_entropy_src;
    auto* g = tb->rootp->__PVT__tb_fi_entropy_src__DOT__u_golden;
    auto* f = tb->rootp->__PVT__tb_fi_entropy_src__DOT__u_fault;

    int force_from = -1; bool force_consumer = false;
    for (int i = 1; i < argc; i++)
        if (!std::strcmp(argv[i], "+forceconsumer")) force_consumer = true;
    for (int i = 1; i < argc; i++)
        if (!std::strncmp(argv[i], "+forcecycle=", 12)) force_from = std::atoi(argv[i] + 12);

    for (int c = 0; c < 120; c++) {
        tb->clk = 0; tb->eval();
        bool inwin = (force_from >= 0 && c >= force_from);
        auto apply = [&]() {
            f->u_entropy_src_core__DOT__rtlsec_fi_sel_mubi_rng_fips = 1;
            f->u_entropy_src_core__DOT__rtlsec_fi_val_mubi_rng_fips = 0;
            if (force_consumer) {
                f->u_entropy_src_core__DOT__rtlsec_fi_sel_rng_fips_pfe = 1;
                f->u_entropy_src_core__DOT__rtlsec_fi_val_rng_fips_pfe = 1;
            }
        };
        if (inwin) apply();
        tb->clk = 1; tb->eval();
        if (inwin) apply();
        if (c % 20 == 0 || (c >= 28 && c <= 34)) {
            std::printf(
                "c=%3d | rng_fips_o g=%d f=%d | pfe g=%d f=%d | pfa g=%d f=%d | "
                "fanout g=0x%x f=0x%x\n",
                c,
                (int)g->rng_fips_o, (int)f->rng_fips_o,
                (int)g->u_entropy_src_core__DOT__rng_fips_pfe,
                (int)f->u_entropy_src_core__DOT__rng_fips_pfe,
                (int)g->u_entropy_src_core__DOT__rng_fips_pfa,
                (int)f->u_entropy_src_core__DOT__rng_fips_pfa,
                (unsigned)g->u_entropy_src_core__DOT__mubi_rng_fips_fanout,
                (unsigned)f->u_entropy_src_core__DOT__mubi_rng_fips_fanout);
            std::printf(
                "        CONSUMER-MUX forced: pfe f=%d  rng_fips_o f=%d (if pfe=1 but o=0 the top port is stale)\n",
                (int)f->u_entropy_src_core__DOT__rng_fips_pfe, (int)f->rng_fips_o);
        }
    }
    return 0;
}
