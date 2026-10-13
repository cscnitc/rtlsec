// tiny.sv — minimal fixture for proving fault injection in Verilator 4.x.
//
// A security-style control: `lock_q` is set once and must never clear. The
// output `locked_o` reports it. A fault that clears `lock_q` is exactly the
// class of failure the tool looks for, in miniature.
module tiny (
  input  logic clk,
  input  logic rst_ni,
  input  logic set_i,
  output logic locked_o,
  output logic err_o
);
  logic lock_q;
  always_ff @(posedge clk or negedge rst_ni) begin
    if (!rst_ni)      lock_q <= 1'b0;
    else if (set_i)   lock_q <= 1'b1;   // sticky: no clear path
  end
  assign locked_o = lock_q;
  assign err_o    = 1'b0;               // the design reports nothing
endmodule

// Top level for the simulation model: the harness drives clk itself.
module tiny_top (
  input logic clk
);
  logic rst_ni = 1'b0;
  logic set_i  = 1'b0;
  logic locked_o;
  logic err_o;
  int unsigned cyc = 0;

  tiny u_dut (
    .clk(clk), .rst_ni(rst_ni), .set_i(set_i),
    .locked_o(locked_o), .err_o(err_o)
  );

  always @(posedge clk) cyc <= cyc + 1;

  // Tiny stimulus, then hold. The C++ driver takes over for the fault.
  initial begin
    #0;
  end

  always @(posedge clk) begin
    if (cyc == 4)  rst_ni <= 1'b1;
    if (cyc == 8)  set_i  <= 1'b1;
    if (cyc == 12) set_i  <= 1'b0;
    if (cyc == 20) begin
      $display("OBS cyc=%0d locked=%0b err=%0b", cyc, locked_o, err_o);
    end
    if (cyc > 200) $finish;
  end
endmodule
