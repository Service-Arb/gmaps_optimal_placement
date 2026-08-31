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

        # `study <path/to/config.nix>` — build the map, then drive it in headless Chromium.
        study = pkgs.writeShellApplication {
          name = "study";
          runtimeInputs = with pkgs; [ rust git pkg-config openssl mold nodejs chromium psmisc nix ];
          text = ''
            cd "$(git rev-parse --show-toplevel)"
            config="''${1:-examples/clermont_detailing/config.nix}"
            out="''${SERVICE_ARB_WORK:-tmp/geo}/out"
            cargo run -p service_arb -- "$config" --out "$out/map.html"
            fuser -k ${toString port}/tcp 2>/dev/null || true
            (cd "$out" && python3 -m http.server ${toString port} >/dev/null 2>&1 &)
            sleep 1
            node examples/clermont_detailing/smoke.js "http://localhost:${toString port}/map.html"
            fuser -k ${toString port}/tcp 2>/dev/null || true
          '';
        };
        # `open <path/to/config.nix>` — build the map, serve it, open in the desktop browser.
        open = pkgs.writeShellApplication {
          name = "open-map";
          runtimeInputs = with pkgs; [ rust git pkg-config openssl mold python3 psmisc xdg-utils nix ];
          text = ''
            cd "$(git rev-parse --show-toplevel)"
            config="''${1:-examples/clermont_detailing/config.nix}"
            out="''${SERVICE_ARB_WORK:-tmp/geo}/out"
            cargo run -p service_arb -- "$config" --out "$out/map.html"
            fuser -k ${toString port}/tcp 2>/dev/null || true
            xdg-open "http://localhost:${toString port}/map.html" &
            cd "$out" && python3 -m http.server ${toString port}
          '';
        };

        help = pkgs.writeShellApplication {
          name = "help";
          text = ''
            cat <<'EOF'
            nix run .#open  [study.nix]  open the built map in your browser (serves on :${toString port}, Ctrl-C to stop)
            nix run .#study [study.nix]  build the map and assert its numbers in headless Chromium
            nix run .#help               this
            the study defaults to examples/clermont_detailing/config.nix; output under $SERVICE_ARB_WORK (default tmp/geo)
            a study is a Nix file evaluating to the attrset `service_arb --schema` describes: area, grid,
            poi (queries + weighted tiers), columns, model, layers, candidates.
            EOF
          '';
        };
      in
      {
        apps = {
          default = { type = "app"; program = pkgs.lib.getExe study; };
          study = { type = "app"; program = pkgs.lib.getExe study; };
          open = { type = "app"; program = pkgs.lib.getExe open; };
          help = { type = "app"; program = pkgs.lib.getExe help; };
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
