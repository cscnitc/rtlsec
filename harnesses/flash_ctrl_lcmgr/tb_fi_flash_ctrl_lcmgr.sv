`timescale 1ns/1ps
// ---------------------------------------------------------------------------
// Differential fault-injection testbench for the REAL OpenTitan
// flash_ctrl_lcmgr module (life-cycle manager).
//
// Two identical instances (u_golden / u_fault) are driven by ONE coherent
// stimulus so they stay in lockstep absent an injected fault.
//
// Key lessons baked in:
//  * Typed control inputs (MuBi4 / lc_tx_t) are driven with CANONICAL
//    CONSTANTS, never random bits.  In particular disable_i MUST be
//    prim_mubi_pkg::MuBi4False (4'b1001); the previous TB tied it to '0
//    (4'b0000), which mubi4_test_true_loose() reads as "disable asserted"
//    -> the FSM parked in StDisabled (2019) forever. That was the false
//    negative, not a clean design.
//  * The OTP key req/ack handshake (prim_sync_reqack across clk_i/clk_otp_i)
//    is modelled: otp_key_req_o is watched and otp_key_rsp_i.{addr,data}_ack
//    (+ key/rand_key) are returned.
//  * The flash read handshake is modelled: while the FSM is in StReadSeeds a
//    transaction is started, SeedReads read beats are returned with CORRECT
//    prim_secded_inv_39_32 integrity (so the design's integrity decoder never
//    fires and intg_err_o stays low), followed by done_i.
//
// Nothing here modifies the DUT; it is pure stimulus.
// ---------------------------------------------------------------------------
module tb_fi_flash_ctrl_lcmgr (
  input logic clk,
  // Witness outputs, read by the C++ driver. Declared as top-level ports so
  // the toolchain cannot inline them away: internal nets get folded into their
  // consumers and vanish from the generated model.
  output logic [10:0]  obs_state_g,
  output logic         obs_req_g,
  output logic         obs_seed_err_g,
  output logic         obs_intg_err_g,
  output logic         obs_fatal_err_g,
  output logic         obs_init_busy_g,
  output logic         obs_initialized_g,
  output logic [511:0] obs_seeds_g,
obs_seeds_f,
  // Reduction over BOTH instances' outputs. Declared as a port (not an
  // internal net printed once) so the toolchain must keep driving it,
  // which in turn keeps the fault instance's output logic alive. As a
  // net whose only use was a one-off $display it could be dropped, and
  // the fault instance's outputs then sat at 0 while the golden
  // instance computed real values - a divergence with no fault injected.
  output logic obs_req_o_g,
  output logic obs_req_o_f,
  output logic [14:0] obs_addr_o_g,
  output logic [14:0] obs_addr_o_f,
  output logic obs_rready_o_g,
  output logic obs_rready_o_f,
  output logic obs_wvalid_o_g,
  output logic obs_wvalid_o_f,
  output logic [38:0] obs_wdata_o_g,
  output logic [38:0] obs_wdata_o_f,
  output logic obs_phase_o_g,
  output logic obs_phase_o_f,
  output logic obs_rd_buf_en_o_g,
  output logic obs_rd_buf_en_o_f,
  output logic obs_edn_req_o_g,
  output logic obs_edn_req_o_f,
  output logic obs_lfsr_en_o_g,
  output logic obs_lfsr_en_o_f,
  output logic obs_init_busy_o_g,
  output logic obs_init_busy_o_f,
  output logic obs_initialized_o_g,
  output logic obs_initialized_o_f,
  output logic [10:0] obs_debug_state_o_g,
  output logic [10:0] obs_debug_state_o_f,
  output logic obs_fatal_err_o_g,
  output logic obs_fatal_err_o_f,
  output logic obs_intg_err_o_g,
  output logic obs_intg_err_o_f,
  output logic obs_seed_err_o_g,
  output logic obs_seed_err_o_f,
  output logic         obs_alive_o
);

  // SeedWidth(256)/BusWidth(32) = 8 bus beats per seed.
  localparam int SeedReads = 8;
  localparam int CYCLES    = 4000;

  // lcmgr_state_e encodings (the enum lives INSIDE flash_ctrl_lcmgr.sv and is
  // not exported through any package, so we mirror it here, verbatim).
  localparam logic [10:0] StIdle          = 11'b10001000001;
  localparam logic [10:0] StReqAddrKey    = 11'b01110101100;
  localparam logic [10:0] StReqDataKey    = 11'b01110010001;
  localparam logic [10:0] StReadSeeds     = 11'b11011111110;
  localparam logic [10:0] StReadEval      = 11'b01000100111;
  localparam logic [10:0] StWait          = 11'b00100111011;
  localparam logic [10:0] StEntropyReseed = 11'b00011000110;
  localparam logic [10:0] StRmaWipe       = 11'b10010110101;
  localparam logic [10:0] StRmaRsp        = 11'b10110001010;
  localparam logic [10:0] StDisabled      = 11'b11111100011;
  localparam logic [10:0] StInvalid       = 11'b11101011000;

  // ------------------------------------------------------------------
  // Clock / reset / cycle counter
  // ------------------------------------------------------------------
  int unsigned cyc = 0;
  logic rst_ni  = 1'b0;
  logic clk_otp = 1'b0;

  always_ff @(posedge clk) begin
    clk_otp <= ~clk_otp;          // clk_i/2 OTP clock domain
    cyc     <= cyc + 1;
    if (cyc == 8)  rst_ni <= 1'b1;
    if (cyc > CYCLES) $finish;
  end

  // ------------------------------------------------------------------
  // Coherent bring-up sequencing (explicit, never LFSR-derived)
  // ------------------------------------------------------------------
  logic init_i_s, provision_en_i_s, edn_ack_i_s;
  lc_ctrl_pkg::lc_tx_t rma_req_i_s;

  always_ff @(posedge clk) begin
    init_i_s         <= (cyc >= 12);   // request init after reset release
    provision_en_i_s <= (cyc >= 12);   // seeds are provisioned
    edn_ack_i_s      <= (cyc >= 200);  // entropy ack (only used on RMA reseed)
    rma_req_i_s      <= lc_ctrl_pkg::Off;
  end

  // ------------------------------------------------------------------
  // DUT output nets
  // ------------------------------------------------------------------
  logic        req_o_g, rready_o_g, wvalid_o_g, phase_o_g;
  logic        fatal_err_o_g, intg_err_o_g, seed_err_o_g, rd_buf_en_o_g, edn_req_o_g;
  logic        lfsr_en_o_g, init_busy_o_g, initialized_o_g;
  logic [10:0] debug_state_o_g;
  logic [14:0] addr_o_g;
  logic [38:0] wdata_o_g;
  logic [511:0] seeds_o_g;   // seed register output (seed-capture witness)

  logic        req_o_f, rready_o_f, wvalid_o_f, phase_o_f;
  logic        fatal_err_o_f, intg_err_o_f, seed_err_o_f, rd_buf_en_o_f, edn_req_o_f;
  logic        lfsr_en_o_f, init_busy_o_f, initialized_o_f;
  logic [10:0] debug_state_o_f;
  logic [14:0] addr_o_f;
  logic [38:0] wdata_o_f;
  logic [511:0] seeds_o_f;

  otp_ctrl_pkg::nvm_otp_key_req_t otp_req_g, otp_req_f;

  // ------------------------------------------------------------------
  // OTP key responder: watch the request, return ack + keys.
  // The design drives otp_key_req_o.{addr_req,data_req} through
  // prim_sync_reqack; we answer in the OTP domain. Acknowledge is
  // combinational on the (already synchronised) request so the NRZ
  // handshake completes; the source side then observes the ack registers.
  // ------------------------------------------------------------------
  logic otp_addr_req, otp_data_req;
  assign otp_addr_req = otp_req_g.addr_req | otp_req_f.addr_req;
  assign otp_data_req = otp_req_g.data_req | otp_req_f.data_req;

  otp_ctrl_pkg::nvm_otp_key_rsp_t otp_rsp;
  always_comb begin
    otp_rsp.data_ack   = otp_data_req;
    otp_rsp.addr_ack   = otp_addr_req;
    otp_rsp.key        = 128'h0123456789abcdef_fedcba9876543210;
    otp_rsp.rand_key   = 128'h0f1e2d3c4b5a6978_8796a5b4c3d2e1f0;
    otp_rsp.seed_valid = 1'b1;
  end

  // ------------------------------------------------------------------
  // Flash (seed) read responder.
  // When the FSM enters StReadSeeds we start a transaction: a 1-cycle
  // latency, SeedReads rvalid_i beats carrying correct-integrity rdata_i,
  // then a done_i pulse. Driven from the GOLDEN state so both instances see
  // identical stimulus.
  // ------------------------------------------------------------------
  logic seed_rd_active;
  assign seed_rd_active = (debug_state_o_g == StReadSeeds);

  logic seed_rd_d;
  always_ff @(posedge clk or negedge rst_ni) begin
    if (!rst_ni) seed_rd_d <= 1'b0;
    else         seed_rd_d <= seed_rd_active;
  end
  logic seed_rd_start;
  assign seed_rd_start = seed_rd_active & ~seed_rd_d;

  typedef enum logic [1:0] {FR_IDLE, FR_LAT, FR_BEAT, FR_DONE} fr_e;
  fr_e fr_q, fr_d;
  logic [4:0] fr_cnt;

  always_comb begin
    fr_d = fr_q;
    unique case (fr_q)
      FR_IDLE: if (seed_rd_start) fr_d = FR_LAT;
      FR_LAT : fr_d = FR_BEAT;
      FR_BEAT: if (fr_cnt == 5'(SeedReads-1)) fr_d = FR_DONE;
      FR_DONE: fr_d = FR_IDLE;
      default: fr_d = FR_IDLE;
    endcase
  end

  always_ff @(posedge clk or negedge rst_ni) begin
    if (!rst_ni) begin
      fr_q   <= FR_IDLE;
      fr_cnt <= '0;
    end else begin
      fr_q <= fr_d;
      if      (fr_q == FR_IDLE) fr_cnt <= '0;
      else if (fr_q == FR_BEAT) fr_cnt <= fr_cnt + 5'(1);
    end
  end

  logic rvalid_s, done_s;
  assign rvalid_s = (fr_q == FR_BEAT);
  assign done_s   = (fr_q == FR_DONE);

  // Read data: enc(data) with prim_secded_inv_39_32.
  //
  // With +bad_intg=1 the INTEGRITY field (upper 7 bits) is deliberately
  // corrupted, producing a codeword the design's decoder must reject. This is
  // the seed-capture precondition violation this pattern targets: the seed-capture path that is
  // suspected of ignoring integrity is only exercised by a read beat carrying
  // BAD integrity, and the original responder always supplied good integrity,
  // so the path was never reached and no fault injection could surface it.
  logic [31:0] fr_data;
  assign fr_data = 32'hA5A5_0000 | {27'b0, fr_cnt};

  logic [6:0] err_val;      // flash_ctrl_err_t presented on err_i
  logic bad_intg, bad_data;
  int unsigned err_start = 0, err_for = 0;
  initial begin
    int unsigned v;
    v = 0;
    void'($value$plusargs("bad_intg=%d", v));
    bad_intg = (v != 0);
    v = 0;
    void'($value$plusargs("bad_data=%d", v));
    bad_data = (v != 0);
    void'($value$plusargs("err_start=%d", err_start));
    void'($value$plusargs("err_for=%d", err_for));
  end

  // err_i is pulsed only inside [err_start, err_start+err_for): afterwards it
  // returns to 0; whether the error output stays asserted is the
  // stickiness question the seed-error pattern raises.
  always_comb begin
    err_val = (err_for != 0 && cyc >= err_start && cyc < err_start + err_for)
              ? 7'h01 : 7'h00;
  end

  logic [38:0] fr_data_enc, fr_rdata;
  prim_secded_inv_39_32_enc u_rdata_enc (
    .data_i (fr_data),
    .data_o (fr_data_enc)
  );
  // +bad_intg corrupts the 7 integrity bits (decoder must reject).
  // +bad_data corrupts the 32 data bits while leaving the codeword valid, so
  // the decoder accepts it: that isolates 'did the capture store the value'
  // from 'did the design notice'.
  assign fr_rdata = (bad_intg ? (fr_data_enc ^ 39'h7F_0000_0000) : fr_data_enc)
                    ^ (bad_data ? 39'h0000_FFFF_FFFF : 39'h0);

  // ------------------------------------------------------------------
  // DUT instances — identical connections / identical stimulus
  // ------------------------------------------------------------------
  flash_ctrl_lcmgr u_golden (
    .clk_i          (clk),
    .rst_ni         (rst_ni),
    .clk_otp_i      (clk_otp),
    .rst_otp_ni     (rst_ni),
    .init_i         (init_i_s),
    .provision_en_i (provision_en_i_s),
    .disable_i      (prim_mubi_pkg::MuBi4False),
    .ctrl_o         (),
    .req_o          (req_o_g),
    .addr_o         (addr_o_g),
    .done_i         (done_s),
    .err_i          (err_val),
    .rready_o       (rready_o_g),
    .rvalid_i       (rvalid_s),
    .wvalid_o       (wvalid_o_g),
    .wready_i       (1'b1),
    .rdata_i        (fr_rdata),
    .wdata_o        (wdata_o_g),
    .rma_req_i      (rma_req_i_s),
    .rma_ack_o      (),
    .seeds_o        (seeds_o_g),
    .phase_o        (phase_o_g),
    .fatal_err_o    (fatal_err_o_g),
    .intg_err_o     (intg_err_o_g),
    .seed_err_o     (seed_err_o_g),
    .rd_buf_en_o    (rd_buf_en_o_g),
    .otp_key_req_o  (otp_req_g),
    .otp_key_rsp_i  (otp_rsp),
    .addr_key_o     (),
    .data_key_o     (),
    .rand_addr_key_o(),
    .rand_data_key_o(),
    .edn_req_o      (edn_req_o_g),
    .edn_ack_i      (edn_ack_i_s),
    .lfsr_en_o      (lfsr_en_o_g),
    .rand_i         (32'h1234_5678),
    .dis_access_o   (),
    .init_busy_o    (init_busy_o_g),
    .initialized_o  (initialized_o_g),
    .debug_state_o  (debug_state_o_g)
  );

  flash_ctrl_lcmgr u_fault (
    .clk_i          (clk),
    .rst_ni         (rst_ni),
    .clk_otp_i      (clk_otp),
    .rst_otp_ni     (rst_ni),
    .init_i         (init_i_s),
    .provision_en_i (provision_en_i_s),
    .disable_i      (prim_mubi_pkg::MuBi4False),
    .ctrl_o         (),
    .req_o          (req_o_f),
    .addr_o         (addr_o_f),
    .done_i         (done_s),
    .err_i          (err_val),
    .rready_o       (rready_o_f),
    .rvalid_i       (rvalid_s),
    .wvalid_o       (wvalid_o_f),
    .wready_i       (1'b1),
    .rdata_i        (fr_rdata),
    .wdata_o        (wdata_o_f),
    .rma_req_i      (rma_req_i_s),
    .rma_ack_o      (),
    .seeds_o        (seeds_o_f),
    .phase_o        (phase_o_f),
    .fatal_err_o    (fatal_err_o_f),
    .intg_err_o     (intg_err_o_f),
    .seed_err_o     (seed_err_o_f),
    .rd_buf_en_o    (rd_buf_en_o_f),
    .otp_key_req_o  (otp_req_f),
    .otp_key_rsp_i  (otp_rsp),
    .addr_key_o     (),
    .data_key_o     (),
    .rand_addr_key_o(),
    .rand_data_key_o(),
    .edn_req_o      (edn_req_o_f),
    .edn_ack_i      (edn_ack_i_s),
    .lfsr_en_o      (lfsr_en_o_f),
    .rand_i         (32'h1234_5678),
    .dis_access_o   (),
    .init_busy_o    (init_busy_o_f),
    .initialized_o  (initialized_o_f),
    .debug_state_o  (debug_state_o_f)
  );

  // TEMPORARY diagnostic: trace the golden FSM so a parked harness is visible
  // rather than silently producing null sweeps.
  always_ff @(posedge clk) begin
    if (cyc < 260)
      $display("TRACE cyc=%0d state=%b req=%b init_busy=%b initialized=%b seed_err=%b phase=%b",
               cyc, debug_state_o_g, req_o_g, init_busy_o_g, initialized_o_g,
               seed_err_o_g, phase_o_g);
    // Per-cycle reference to BOTH instances' outputs. Elimination is decided
    // from Verilog usage alone, so a net referenced only once (or only by an
    // output nothing reads) can be dropped, leaving that instance's output at 0
    // while its twin computes real values - a divergence with no fault injected.
    // Reading them every cycle for a window keeps both cones alive.
    if (cyc < 300)
      $display("KEEP g=%b %b %b %b f=%b %b %b %b",
               req_o_g, wvalid_o_g, rready_o_g, phase_o_g,
               req_o_f, wvalid_o_f, rready_o_f, phase_o_f);
  end

  // Witness ports (see header): driven straight from the golden instance.
  assign obs_state_g       = debug_state_o_g;
  assign obs_req_g         = req_o_g;
  assign obs_seed_err_g    = seed_err_o_g;
  assign obs_intg_err_g    = intg_err_o_g;
  assign obs_fatal_err_g   = fatal_err_o_g;
  assign obs_init_busy_g   = init_busy_o_g;
  assign obs_initialized_g = initialized_o_g;
  assign obs_seeds_g       = seeds_o_g;
assign obs_seeds_f       = seeds_o_f;

  // Paired per-instance witness ports (the oracle compares these, not DUT members).
  assign obs_req_o_g = req_o_g;
  assign obs_req_o_f = req_o_f;
  assign obs_addr_o_g = addr_o_g;
  assign obs_addr_o_f = addr_o_f;
  assign obs_rready_o_g = rready_o_g;
  assign obs_rready_o_f = rready_o_f;
  assign obs_wvalid_o_g = wvalid_o_g;
  assign obs_wvalid_o_f = wvalid_o_f;
  assign obs_wdata_o_g = wdata_o_g;
  assign obs_wdata_o_f = wdata_o_f;
  assign obs_phase_o_g = phase_o_g;
  assign obs_phase_o_f = phase_o_f;
  assign obs_rd_buf_en_o_g = rd_buf_en_o_g;
  assign obs_rd_buf_en_o_f = rd_buf_en_o_f;
  assign obs_edn_req_o_g = edn_req_o_g;
  assign obs_edn_req_o_f = edn_req_o_f;
  assign obs_lfsr_en_o_g = lfsr_en_o_g;
  assign obs_lfsr_en_o_f = lfsr_en_o_f;
  assign obs_init_busy_o_g = init_busy_o_g;
  assign obs_init_busy_o_f = init_busy_o_f;
  assign obs_initialized_o_g = initialized_o_g;
  assign obs_initialized_o_f = initialized_o_f;
  assign obs_debug_state_o_g = debug_state_o_g;
  assign obs_debug_state_o_f = debug_state_o_f;
  assign obs_fatal_err_o_g = fatal_err_o_g;
  assign obs_fatal_err_o_f = fatal_err_o_f;
  assign obs_intg_err_o_g = intg_err_o_g;
  assign obs_intg_err_o_f = intg_err_o_f;
  assign obs_seed_err_o_g = seed_err_o_g;
  assign obs_seed_err_o_f = seed_err_o_f;

  // Keep every observed output ALIVE. Verilator eliminates logic whose result
  // is never read, and the fault instance's outputs are read by nothing in this
  // testbench, so its driving logic gets deleted and it sits at zero while the
  // golden instance computes real values. The differential then reports a
  // divergence with no fault injected at all. Referencing both instances here
  // (and printing the reduction) keeps both chains live.
  always_comb obs_alive_o = ^{
    req_o_g, req_o_f, addr_o_g, addr_o_f, rready_o_g, rready_o_f,
    wvalid_o_g, wvalid_o_f, wdata_o_g, wdata_o_f, phase_o_g, phase_o_f,
    fatal_err_o_g, fatal_err_o_f, intg_err_o_g, intg_err_o_f,
    seed_err_o_g, seed_err_o_f, rd_buf_en_o_g, rd_buf_en_o_f,
    edn_req_o_g, edn_req_o_f, lfsr_en_o_g, lfsr_en_o_f,
    init_busy_o_g, init_busy_o_f, initialized_o_g, initialized_o_f,
    debug_state_o_g, debug_state_o_f,
    seeds_o_g, seeds_o_f
  };

endmodule
