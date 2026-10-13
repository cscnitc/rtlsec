// Is the observable stale at the DUT member, or is the TB wire correct?
#include "Vtb_fi_entropy_src.h"
#include "Vtb_fi_entropy_src___024root.h"
#include "Vtb_fi_entropy_src_entropy_src.h"
#include "verilated.h"
#include <cstdio>
#include <cstring>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    auto* tb = new Vtb_fi_entropy_src;
    auto* f = tb->rootp->__PVT__tb_fi_entropy_src__DOT__u_fault;
    auto* g = tb->rootp->__PVT__tb_fi_entropy_src__DOT__u_golden;

    int from = -1;
    for (int i = 1; i < argc; i++)
        if (!std::strncmp(argv[i], "+forcecycle=", 12)) from = std::atoi(argv[i] + 12);

    for (int c = 0; c < 60; c++) {
        tb->clk = 0; tb->eval();
        bool inwin = (from >= 0 && c >= from);
        if (inwin) {
            f->u_entropy_src_core__DOT__rtlsec_fi_sel_mubi_rng_fips = 1;
            f->u_entropy_src_core__DOT__rtlsec_fi_val_mubi_rng_fips = 0;
        }
        tb->clk = 1; tb->eval();
        if (inwin) {
            f->u_entropy_src_core__DOT__rtlsec_fi_sel_mubi_rng_fips = 1;
            f->u_entropy_src_core__DOT__rtlsec_fi_val_mubi_rng_fips = 0;
        }
        if (c >= 28 && c <= 34) {
            std::printf("c=%2d pfe_f=%d | DUT member rng_fips_o: g=%d f=%d | TB wire rng_fips_o_f=%d g=%d\n",
                c,
                (int)f->u_entropy_src_core__DOT__rng_fips_pfe,
                (int)g->rng_fips_o, (int)f->rng_fips_o,
                (int)tb->rootp->__PVT__tb_fi_entropy_src__DOT__rng_fips_o_f,
                (int)tb->rootp->__PVT__tb_fi_entropy_src__DOT__rng_fips_o_g);
        }
    }
    return 0;
}
