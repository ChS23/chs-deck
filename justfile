id          := "chs.deck.sdPlugin"
plugins_dir := "/home/chs/.config/opendeck/plugins"
profile_dir := "/home/chs/.config/opendeck/profiles/99-4250D2781310"

# Build release binary
build:
    cargo build --release --target x86_64-unknown-linux-gnu --target-dir target/plugin

# Deploy plugin files (no rebuild)
deploy:
    rm -rf {{plugins_dir}}/{{id}}
    mkdir -p {{plugins_dir}}/{{id}}/assets
    cp manifest.json        {{plugins_dir}}/{{id}}/
    cp assets/*.png         {{plugins_dir}}/{{id}}/assets/
    cp assets/pi-stats.html  {{plugins_dir}}/{{id}}/assets/
    cp assets/pi-shell.html  {{plugins_dir}}/{{id}}/assets/
    cp assets/pi-media.html  {{plugins_dir}}/{{id}}/assets/
    cp target/plugin/x86_64-unknown-linux-gnu/release/chs-deck {{plugins_dir}}/{{id}}/chs-deck-linux
    @echo "Deployed to {{plugins_dir}}/{{id}}"

# Build + deploy
install: build deploy

# Generate one profile JSON from profiles/<name>.toml  (default: work)
gen-profile name="work":
    cargo run --bin gen_profile -- {{name}} 2>/dev/null > {{profile_dir}}/{{name}}.json
    @echo "Profile '{{name}}' written"

# Regenerate ALL profiles (nav buttons get correct ring links)
gen-profiles:
    #!/usr/bin/env bash
    set -e
    for f in profiles/*.toml; do
      name=$(basename "$f" .toml)
      cargo run --bin gen_profile -- "$name" 2>/dev/null > "{{profile_dir}}/$name.json"
      echo "  ✓ $name"
    done
    echo "All profiles written to {{profile_dir}}"

# Kill OpenDeck → write profile → relaunch
reload-profile name="work": (gen-profile name)
    @echo "Stopping OpenDeck..."
    pkill -x opendeck || true
    sleep 1
    @echo "Launching OpenDeck..."
    opendeck &
    @echo "Done"

# Full cycle: build + deploy + reload profile
release: install reload-profile

package: build collect zip

collect:
    rm -rf build
    mkdir -p build/{{id}}
    cp -r assets build/{{id}}
    cp manifest.json build/{{id}}
    cp target/plugin/x86_64-unknown-linux-gnu/release/chs-deck build/{{id}}/chs-deck-linux

[working-directory: "build"]
zip:
    zip -r chs-deck.plugin.zip {{id}}/
