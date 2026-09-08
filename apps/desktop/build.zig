const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const desktop = b.addExecutable(.{
        .name = "makima-desktop",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });

    const ghostty = b.dependency("ghostty", .{
        .target = target,
        .optimize = optimize,
    });
    desktop.root_module.addImport("ghostty-vt", ghostty.module("ghostty-vt"));

    b.installArtifact(desktop);

    const run = b.addRunArtifact(desktop);
    run.step.dependOn(b.getInstallStep());
    if (b.args) |args| run.addArgs(args);

    const run_step = b.step("run", "Run the desktop scaffold");
    run_step.dependOn(&run.step);
}
