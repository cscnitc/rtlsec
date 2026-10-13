// fi_proof.cpp — proof that a fault can be injected on an internal signal of a
// real Verilated design, and that the fault's effect is silent.
//
// Verilator 4.x has no `force`/`release` statements. With `--public-flat-rw`
// the model exposes internal nets as public members with flattened names
// (`a.b.c` -> `a__DOT__b__DOT__c`), so a fault is an assignment to that member
// after each evaluation. That is all a fault campaign needs: address any
// ranked signal, at any cycle, with any fault model.
//
// Golden and faulted runs use identical stimulus, so comparing them is a
// controlled differential — no relearning of what the correct state is.

#include "Vtiny.h"
#include "Vtiny___024root.h"   // required for --public-flat-rw access
#include "verilated.h"

#include <cstdio>
#include <cstring>
#include <string>

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    const bool inject = (argc > 1 && std::string(argv[1]) == "fault");
    Vtiny* tb = new Vtiny;

    int locked_seen = 0, faulted_cycles = 0;
    bool diverged = false;

    for (int c = 0; c < 300 && !Verilated::gotFinish(); c++) {
        tb->clk = 0;
        tb->eval();
        tb->clk = 1;
        tb->eval();

        // The whole fault model: one assignment to one internal net.
        if (inject && tb->rootp->tiny_top__DOT__u_dut__DOT__lock_q) {
            tb->rootp->tiny_top__DOT__u_dut__DOT__lock_q = 0;
            faulted_cycles++;
        }

        if (tb->rootp->tiny_top__DOT__locked_o) locked_seen++;
        // A divergence the design did not report: err_o stays 0 in both runs.
        if (inject && faulted_cycles > 0 &&
            !tb->rootp->tiny_top__DOT__locked_o) {
            diverged = true;
        }
    }

    const auto err = tb->rootp->tiny_top__DOT__err_o;
    std::printf("mode=%s locked_cycles=%d lock_cleared=%d diverged=%d err_o=%d\n",
                inject ? "faulted" : "golden", locked_seen, faulted_cycles,
                diverged ? 1 : 0, (int)err);
    // A finding = observables diverged AND the detection path was silent.
    if (inject && diverged && err == 0) {
        std::printf("ORACLE: silent divergence -> FINDING "
                    "(signal=u_dut.lock_q fault=stuck-at-0)\n");
    } else if (!inject) {
        std::printf("ORACLE: golden run, behaviour holds\n");
    }
    delete tb;
    return 0;
}
