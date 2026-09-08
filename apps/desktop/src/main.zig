const std = @import("std");
const ghostty = @import("ghostty-vt");

pub fn main(init: std.process.Init) !void {
    var terminal: ghostty.Terminal = try .init(init.io, init.gpa, .{
        .cols = 80,
        .rows = 24,
    });
    defer terminal.deinit(init.gpa);

    try terminal.printString("Hello from Makima and libghostty!");

    const screen = try terminal.plainString(init.gpa);
    defer init.gpa.free(screen);
    std.debug.print("{s}\n", .{screen});
}
