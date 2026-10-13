#!/usr/bin/env bash
# Build the include-path and prelude lists used for the OpenTitan run.
set -euo pipefail
OT=${OT:-${RTL_TREE:-.}}

inc_dirs() {
  ls -d "$OT"/hw/ip/*/rtl 2>/dev/null
  ls -d "$OT"/hw/top_earlgrey/ip_autogen/*/rtl 2>/dev/null
  ls -d "$OT"/hw/ip/prim/rtl 2>/dev/null
  ls -d "$OT"/hw/top_earlgrey/rtl 2>/dev/null
}

prelude_files() {
  # Files whose `define macros are used without being included by the users.
  ls "$OT"/hw/ip/prim/rtl/prim_assert.sv 2>/dev/null
  ls "$OT"/hw/ip/prim/rtl/prim_assert_sec_cm.svh 2>/dev/null
  ls "$OT"/hw/ip/prim/rtl/prim_flop_macros.sv 2>/dev/null
  ls "$OT"/hw/ip/prim/rtl/*.sv 2>/dev/null
}
