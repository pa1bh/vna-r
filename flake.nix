{
  description = "vna-r: LiteVNA / NanoVNA-F tooling (library, CLI, egui GUI)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAll (pkgs:
        let
          inherit (pkgs) lib stdenv;
          # What the GUI dlopens at run time on Linux: winit (X11 / Wayland) and
          # wgpu (Vulkan, with GL as fallback), and rfd (libdbus, to reach the
          # xdg desktop portal for the file dialogs).
          runtimeLibs = lib.optionals stdenv.hostPlatform.isLinux (with pkgs; [
            dbus
            libxkbcommon
            wayland
            libGL
            vulkan-loader
            libx11
            libxcursor
            libxi
            libxrandr
          ]);
          # One version number for the whole workspace, read from Cargo.toml.
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
          vna-r = pkgs.rustPlatform.buildRustPackage {
            pname = "vna-r";
            inherit version;
            # build.rs shows this in About and --version; the store copy has no .git.
            VNA_R_GIT_REV = self.shortRev or self.dirtyShortRev or "unknown";
            # Only the git-tracked files, so a local target/ or cal/ never ends up in the store.
            src = self;
            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [ pkgs.pkg-config ]
              ++ lib.optionals stdenv.hostPlatform.isLinux [ pkgs.makeWrapper ];
            buildInputs = lib.optionals stdenv.hostPlatform.isLinux [ pkgs.udev ];

            # The tests need no hardware.
            doCheck = true;

            # Ship the udev rule so `services.udev.packages = [ vna-r ]` works on NixOS,
            # and give the GUI the graphics libraries on its search path.
            postInstall = lib.optionalString stdenv.hostPlatform.isLinux ''
              install -Dm644 linux/70-litevna.rules $out/lib/udev/rules.d/70-litevna.rules
              wrapProgram $out/bin/vna-gui --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
            '';

            meta = with lib; {
              description = "Drive a LiteVNA or NanoVNA-F from the desktop: sweeps, SOL(T) calibration, TDR, gating";
              homepage = "https://github.com/pa1bh/vna-r";
              license = licenses.mit;
              platforms = platforms.unix;
              mainProgram = "vna-gui";
            };
          };
        in
        {
          default = vna-r;
          inherit vna-r;
        });

      apps = forAll (pkgs:
        let pkg = self.packages.${pkgs.stdenv.hostPlatform.system}.default; in
        {
          default = { type = "app"; program = "${pkg}/bin/vna-gui"; };
          vna-gui = { type = "app"; program = "${pkg}/bin/vna-gui"; };
          vna-cli = { type = "app"; program = "${pkg}/bin/vna-cli"; };
        });

      devShells = forAll (pkgs:
        let
          inherit (pkgs) lib stdenv;
          runtimeLibs = lib.optionals stdenv.hostPlatform.isLinux (with pkgs; [
            dbus libxkbcommon wayland libGL vulkan-loader
            libx11 libxcursor libxi libxrandr
          ]);
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [ rustc cargo rustfmt clippy rust-analyzer pkg-config ]
              ++ lib.optionals stdenv.hostPlatform.isLinux [ pkgs.udev ];
            # `cargo run -p vna-gui` finds the graphics libraries through this.
            LD_LIBRARY_PATH = lib.optionalString stdenv.hostPlatform.isLinux (lib.makeLibraryPath runtimeLibs);
          };
        });
    };
}
