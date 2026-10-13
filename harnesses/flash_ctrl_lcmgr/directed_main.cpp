// directed_main.cpp — DIRECTED precondition-violation test for the seed path.
//
// Not a fault-injection campaign. This asks a behavioural question that fault
// injection cannot express: when the flash controller returns a read beat with
// BAD integrity during the seed fetch, does the design (a) refuse to capture it
// and/or (b) report the error? the pattern under test is whether the capture happens regardless.
//
// Run twice and compare:
//   ./bin +bad_intg=0   good integrity (the happy path the old responder only ever gave)
//   ./bin +bad_intg=1   integrity field corrupted, decoder must reject
//
// Witnesses: seed_err_o / intg_err_o / fatal_err_o (did the design notice?), and
// a hash of the seeds_o output (did the seed register actually change?).
#include "Vtb_fi_flash_ctrl_lcmgr.h"
#include "Vtb_fi_flash_ctrl_lcmgr___024root.h"
#include "Vtb_fi_flash_ctrl_lcmgr_flash_ctrl_lcmgr.h"
#include "verilated.h"
#include <cstdio>
#include <cstring>
#include <set>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);

    const char* tag = "baseline-clean";
    for (int i = 1; i < argc; i++) {
        if (!std::strncmp(argv[i], "+bad_intg=1", 11)) tag = "BAD-integrity";
        else if (!std::strncmp(argv[i], "+bad_data=1", 11)) tag = "BAD-data";
    }

    Vtb_fi_flash_ctrl_lcmgr* tb = new Vtb_fi_flash_ctrl_lcmgr;
    (void)tb;   // witnesses come from the TB's top-level output ports

    bool seed_err = false, intg_err = false, fatal_err = false;
    bool init_busy_seen = false, req_seen = false, rvalid_seen = false;
    bool initialized_seen = false; long long seed_phase_cycles = 0;
    bool seed_err_last = false; bool seed_err_rose = false;
    std::set<unsigned> states;
    std::set<unsigned long long> seen;
    unsigned long long prev = 0;
    bool first = true;
    long long changes = 0;
    long long seed_phase_seen = 0;

    for (long long c = 0; c < 2000000 && !Verilated::gotFinish(); c++) {
        tb->clk = 0; tb->eval();
        tb->clk = 1; tb->eval();

        if (tb->obs_seed_err_g) { seed_err = true; seed_err_rose = true; }
        seed_err_last = tb->obs_seed_err_g;   // final value: stickiness witness
        if (tb->obs_intg_err_g) intg_err = true;
        if (tb->obs_fatal_err_g) fatal_err = true;
        // init_busy_o == seed_phase inside the DUT, so this proves the seed-read
        // window was actually reached. req_o is driven from seed_phase|rma_phase.
        if (tb->obs_init_busy_g) { init_busy_seen = true; seed_phase_cycles++; }
        if (tb->obs_initialized_g) initialized_seen = true;
        if (tb->obs_req_g) req_seen = true;
        states.insert((unsigned)tb->obs_state_g);

        // hash the seed register (512 bits): a change means a capture happened
        unsigned long long h = 1469598103934665603ULL;
        const unsigned char* p = (const unsigned char*)&tb->obs_seeds_g;
        for (size_t i = 0; i < sizeof(tb->obs_seeds_g); i++) {
            h ^= p[i];
            h *= 1099511628211ULL;
        }
        seen.insert(h);
        if (!first && h != prev) changes++;
        prev = h;
        first = false;
    }

    std::printf("SCENARIO %-15s seed_err_ever=%d seed_err_FINAL=%d intg_err_o=%d fatal_err_o=%d "
                "seed_phase_cycles=%lld initialized_seen=%d req_o_seen=%d states_visited=%zu "
                "seeds_distinct=%zu seeds_changes=%lld final_seeds_hash=%016llx\n",
                tag, seed_err ? 1 : 0, seed_err_last ? 1 : 0, intg_err ? 1 : 0, fatal_err ? 1 : 0,
                seed_phase_cycles, initialized_seen ? 1 : 0, req_seen ? 1 : 0, states.size(),
                seen.size(), changes, prev);
    (void)seed_phase_seen;
    (void)rvalid_seen;
    delete tb;
    return 0;
}
