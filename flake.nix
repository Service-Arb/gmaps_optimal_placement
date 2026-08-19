{
  inputs = {
    v_flakes.url = "github:valeratrades/v_flakes?ref=v1.6";
  };
  outputs = { self, v_flakes }:
    let
      inherit (v_flakes) flake-utils pre-commit-hooks;
    in
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import v_flakes.default_nixpkgs { inherit system; config.allowUnfree = true; };
        rust = v_flakes.rs.default_nightly system;
        pre-commit-check = pre-commit-hooks.lib.${system}.run (v_flakes.files.preCommit { inherit pkgs; stripClaudeSignature = true; });
        manifest = (pkgs.lib.importTOML ./service_arb/Cargo.toml).package;
        pname = manifest.name;
        stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.stdenv;
        # The smoke test serves the built map over http: a `file://` referrer is not something a
        # Maps key can be restricted to.
        port = 8731;

        rs = v_flakes.rs { inherit pkgs rust; };
        github = v_flakes.github {
          inherit pkgs pname rs;
          enable = true;
          lastSupportedVersion = "nightly-2026-07-14";
          jobs.default = true;
          lfs = false;
        };
        readme = v_flakes.readme-fw {
          inherit pkgs pname;
          defaults = true;
          lastSupportedVersion = "nightly-1.92";
          rootDir = ./.;
          badges = [ "msrv" "loc" "ci" ];
        };
        combined = v_flakes.utils.combine { inherit rust; modules = [ rs github readme ]; };

        # `study <path/to/config.toml>` — build the map, then drive it in headless Chromium.
        study = pkgs.writeShellApplication {
          name = "study";
          runtimeInputs = with pkgs; [ rust git pkg-config openssl mold nodejs chromium psmisc ];
          text = ''
            cd "$(git rev-parse --show-toplevel)"
            config="''${1:-examples/clermont_detailing/config.toml}"
            cargo run -p service_arb -- "$config"
            name="$(basename "$config" .toml)"
            out="''${SERVICE_ARB_WORK:-tmp/geo}/out"
            fuser -k ${toString port}/tcp 2>/dev/null || true
            (cd "$out" && python3 -m http.server ${toString port} >/dev/null 2>&1 &)
            sleep 1
            node examples/clermont_detailing/smoke.js "http://localhost:${toString port}/$name.html"
            fuser -k ${toString port}/tcp 2>/dev/null || true
          '';
        };
      in
      {
        apps = {
          default = { type = "app"; program = pkgs.lib.getExe study; };
          study = { type = "app"; program = pkgs.lib.getExe study; };
        };

        packages.default =
          (pkgs.makeRustPlatform { rustc = rust; cargo = rust; inherit stdenv; }).buildRustPackage {
            inherit pname;
            version = manifest.version;
            buildInputs = with pkgs; [ openssl.dev ];
            nativeBuildInputs = with pkgs; [ pkg-config ];
            cargoLock.lockFile = ./Cargo.lock;
            src = pkgs.lib.cleanSource ./.;
          };

        devShells.default =
          with pkgs;
          mkShell {
            inherit stdenv;
            shellHook =
              pre-commit-check.shellHook
              + combined.shellHook
              + ''
                cp -f ${(v_flakes.files.treefmt) { inherit pkgs; }} ./.treefmt.toml
                cp -f ${(v_flakes.files.gitattributes) { inherit pkgs; lfs = false; }} ./.gitattributes
              '';

            packages = [
              chromium
              mold
              nodejs
              openssl
              pkg-config
              python3
              (v_flakes.qlty system)
              rust
              study
            ] ++ pre-commit-check.enabledPackages ++ combined.enabledPackages;

            env.RUST_BACKTRACE = 1;
          };
      }
    );
}
