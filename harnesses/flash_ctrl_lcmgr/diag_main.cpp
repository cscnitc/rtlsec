#include "Vtb_fi_flash_ctrl_lcmgr.h"
#include "Vtb_fi_flash_ctrl_lcmgr___024root.h"
#include "Vtb_fi_flash_ctrl_lcmgr_flash_ctrl_lcmgr.h"
#include "verilated.h"
#include <cstdio>
#include <cstring>
int main(int argc, char** argv) {
  Verilated::commandArgs(argc, argv);
  auto* tb = new Vtb_fi_flash_ctrl_lcmgr;
  auto* g = tb->rootp->__PVT__tb_fi_flash_ctrl_lcmgr__DOT__u_golden;
  auto* f = tb->rootp->__PVT__tb_fi_flash_ctrl_lcmgr__DOT__u_fault;
  bool done = false;
  for (long long c = 0; c < 2000000 && !Verilated::gotFinish() && !done; c++) {
    tb->clk = 0; tb->eval(); tb->clk = 1; tb->eval();
    bool any = false;
    if (sizeof(g->req_o) <= 8 && std::memcmp(&g->req_o, &f->req_o, sizeof(g->req_o)) != 0) { any = true; std::printf("  DIFF req_o: g=%llu f=%llu\n", (unsigned long long)g->req_o, (unsigned long long)f->req_o); }
    if (sizeof(g->addr_o) <= 8 && std::memcmp(&g->addr_o, &f->addr_o, sizeof(g->addr_o)) != 0) { any = true; std::printf("  DIFF addr_o: g=%llu f=%llu\n", (unsigned long long)g->addr_o, (unsigned long long)f->addr_o); }
    if (sizeof(g->__PVT__rready_o) <= 8 && std::memcmp(&g->__PVT__rready_o, &f->__PVT__rready_o, sizeof(g->__PVT__rready_o)) != 0) { any = true; std::printf("  DIFF __PVT__rready_o: g=%llu f=%llu\n", (unsigned long long)g->__PVT__rready_o, (unsigned long long)f->__PVT__rready_o); }
    if (sizeof(g->wvalid_o) <= 8 && std::memcmp(&g->wvalid_o, &f->wvalid_o, sizeof(g->wvalid_o)) != 0) { any = true; std::printf("  DIFF wvalid_o: g=%llu f=%llu\n", (unsigned long long)g->wvalid_o, (unsigned long long)f->wvalid_o); }
    if (sizeof(g->wdata_o) <= 8 && std::memcmp(&g->wdata_o, &f->wdata_o, sizeof(g->wdata_o)) != 0) { any = true; std::printf("  DIFF wdata_o: g=%llu f=%llu\n", (unsigned long long)g->wdata_o, (unsigned long long)f->wdata_o); }
    if (sizeof(g->phase_o) <= 8 && std::memcmp(&g->phase_o, &f->phase_o, sizeof(g->phase_o)) != 0) { any = true; std::printf("  DIFF phase_o: g=%llu f=%llu\n", (unsigned long long)g->phase_o, (unsigned long long)f->phase_o); }
    if (sizeof(g->rd_buf_en_o) <= 8 && std::memcmp(&g->rd_buf_en_o, &f->rd_buf_en_o, sizeof(g->rd_buf_en_o)) != 0) { any = true; std::printf("  DIFF rd_buf_en_o: g=%llu f=%llu\n", (unsigned long long)g->rd_buf_en_o, (unsigned long long)f->rd_buf_en_o); }
    if (sizeof(g->edn_req_o) <= 8 && std::memcmp(&g->edn_req_o, &f->edn_req_o, sizeof(g->edn_req_o)) != 0) { any = true; std::printf("  DIFF edn_req_o: g=%llu f=%llu\n", (unsigned long long)g->edn_req_o, (unsigned long long)f->edn_req_o); }
    if (sizeof(g->lfsr_en_o) <= 8 && std::memcmp(&g->lfsr_en_o, &f->lfsr_en_o, sizeof(g->lfsr_en_o)) != 0) { any = true; std::printf("  DIFF lfsr_en_o: g=%llu f=%llu\n", (unsigned long long)g->lfsr_en_o, (unsigned long long)f->lfsr_en_o); }
    if (sizeof(g->init_busy_o) <= 8 && std::memcmp(&g->init_busy_o, &f->init_busy_o, sizeof(g->init_busy_o)) != 0) { any = true; std::printf("  DIFF init_busy_o: g=%llu f=%llu\n", (unsigned long long)g->init_busy_o, (unsigned long long)f->init_busy_o); }
    if (sizeof(g->initialized_o) <= 8 && std::memcmp(&g->initialized_o, &f->initialized_o, sizeof(g->initialized_o)) != 0) { any = true; std::printf("  DIFF initialized_o: g=%llu f=%llu\n", (unsigned long long)g->initialized_o, (unsigned long long)f->initialized_o); }
    if (sizeof(g->debug_state_o) <= 8 && std::memcmp(&g->debug_state_o, &f->debug_state_o, sizeof(g->debug_state_o)) != 0) { any = true; std::printf("  DIFF debug_state_o: g=%llu f=%llu\n", (unsigned long long)g->debug_state_o, (unsigned long long)f->debug_state_o); }
    if (sizeof(g->fatal_err_o) <= 8 && std::memcmp(&g->fatal_err_o, &f->fatal_err_o, sizeof(g->fatal_err_o)) != 0) { any = true; std::printf("  DIFF fatal_err_o: g=%llu f=%llu\n", (unsigned long long)g->fatal_err_o, (unsigned long long)f->fatal_err_o); }
    if (sizeof(g->intg_err_o) <= 8 && std::memcmp(&g->intg_err_o, &f->intg_err_o, sizeof(g->intg_err_o)) != 0) { any = true; std::printf("  DIFF intg_err_o: g=%llu f=%llu\n", (unsigned long long)g->intg_err_o, (unsigned long long)f->intg_err_o); }
    if (sizeof(g->seed_err_o) <= 8 && std::memcmp(&g->seed_err_o, &f->seed_err_o, sizeof(g->seed_err_o)) != 0) { any = true; std::printf("  DIFF seed_err_o: g=%llu f=%llu\n", (unsigned long long)g->seed_err_o, (unsigned long long)f->seed_err_o); }
    if (any) { std::printf("FIRST_DIVERGENT_CYCLE=%lld (all signals above)\n", c); done = true; }
  }
  if (!done) std::printf("NO_DIVERGENCE\n");
  delete tb; return 0;
}
