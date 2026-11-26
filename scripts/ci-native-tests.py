#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run registered native fixtures from one Cargo-built library test executable.

Every GTK fixture gets a fresh process, X server and session bus: GTK main-thread
ownership and fixture-owned Openbox/MPRIS names must not leak between tests.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time


BUS = [
    "native_mpris_session_bus_methods_properties_signals_and_shutdown",
    "native_secure_storage_secret_service_and_portal_protocol",
    "native_flatpak_update_portal_monitor_and_transaction_protocol",
]
WEBKIT = [
    "webkit_auth_construction_and_shutdown_without_navigation",
    "webkit_auth_local_startup_cookie_isolation_and_cancel",
    "webkit_auth_interactive_navigation_frames_and_first_rejection",
]
FUNCTIONAL = [
    "native_bundled_typography",
    "native_bundled_symbolic_icons_recolor",
    "native_update_dialog_adaptive_status_and_notes",
    "artwork_thumbnail_maps_caches_hidpi_and_cancels_without_network",
    "native_account_menu_logout_cancel_default_confirm_and_stale_session",
    "native_cover_metadata_http_cache_and_render_lifecycle",
    "native_visible_cover_retries_after_worker_queue_saturation_without_remap",
    "native_workspace_track_pane_actions_state_and_cleanup",
    "native_catalog_native_music_layout",
    "native_catalog_navigation_latest_actions",
    "native_content_refresh_routes_and_coalescing",
    "native_library_interactions_confirmed_fixture",
    "native_catalog_rows_mouse_keyboard_details_and_delivery",
    "native_search_retry_normalized_results_and_density",
    "native_global_search_measured_slot_and_motion",
    "native_shortcuts_dispatch_and_preserve_widget_input",
    "native_music_session_catalog_queue_and_logout",
    "native_music_context_mpris_shared_player_and_cleanup",
    "native_wave_shared_player_feedback_batches_and_cleanup",
    "native_wave_retirement_supported_feedback_and_cleanup",
    "native_wave_home_experiment_layout_and_controls",
    "native_music_playback_context_occurrences_and_shared_transport",
    "native_music_player_progress_duration_transitions",
    "native_music_playback_media_error_visibility_stop_retry_and_catalog_preservation",
    "native_music_playback_queue_races_remove_and_close",
    "native_music_playback_resolver_queue_controls_and_logout",
    "native_collection_filter_continuation_and_exact_occurrences",
    "native_music_shuffle_repeat_mpris_shared_policy_and_eos",
    "native_queue_space_mouse_enter_dispatch_exact_occurrence_once",
    "native_workspace_playlist_grid_input_states_and_resize",
    "native_workspace_full_shell_adaptive_pane_selection_navigation_and_cleanup",
    "native_preferences_persistent_controls_snapshot_updates_and_cleanup",
    "native_playlist_creation_confirmation_refresh_refusal_and_late_widgets",
    "native_persistent_session_restore_settings_forget_and_close",
    "native_audio_cache_application_process_restart",
    "native_v07_playback_restore_playlist_appearance_process_restart",
    "native_v08_lyrics_similar_seek_and_stale_selection",
    "native_v08_footer_lyrics_and_volume_scroll",
    "native_music_resume_expired_source_preserves_occurrence_position_and_cancellation",
    "native_v08_whole_collection_index_search_sort_and_cancellation",
    "native_v08_queue_next_drag_keyboard_and_restoration",
    "native_v08_incremental_marker_updates_and_stable_ticks",
    "native_catalog_cache_provenance_interleaving_and_retired_replies",
]
VISUAL = [
    "native_first_footer_reveal_after_startup_without_graphics_initialization",
    "native_design_matched_matrix_and_populated_surfaces",
    "native_universal_panel_whole_surface_motion_and_mode_switch",
    "native_system_visual_surfaces",
]
# Targeted second-scale regressions, rather than replaying every account/network
# fixture. The full native functional suite above still runs on every PR.
HIDPI = [
    "native_bundled_typography",
    "native_bundled_symbolic_icons_recolor",
    "native_update_dialog_adaptive_status_and_notes",
    "native_music_player_progress_duration_transitions",
    "native_music_playback_context_occurrences_and_shared_transport",
    "native_workspace_full_shell_adaptive_pane_selection_navigation_and_cleanup",
    "native_global_search_measured_slot_and_motion",
    "native_v08_lyrics_similar_seek_and_stale_selection",
    "native_v08_footer_lyrics_and_volume_scroll",
    "native_search_retry_normalized_results_and_density",
    "artwork_thumbnail_maps_caches_hidpi_and_cancels_without_network",
]
COMPATIBILITY = [
    "native_bundled_typography",
    "native_bundled_symbolic_icons_recolor",
    "native_music_player_progress_duration_transitions",
    "native_catalog_native_music_layout",
    "native_music_playback_resolver_queue_controls_and_logout",
]
GROUPS = {"bus": BUS, "webkit": WEBKIT, "functional": FUNCTIONAL,
          "hidpi": HIDPI, "compatibility": COMPATIBILITY, "visual": VISUAL,
          "lyrics": ["native_v08_lyrics_similar_seek_and_stale_selection"],
          "high-contrast": ["native_system_visual_surfaces", "native_bundled_typography",
                            "native_bundled_symbolic_icons_recolor"]}


def registered(binary):
    output = subprocess.check_output([binary, "--ignored", "--list"], text=True)
    tests = [line.removesuffix(": test") for line in output.splitlines()
             if line.endswith(": test")]
    result = {name.rsplit("::", 1)[-1]: name for name in tests}
    assert len(result) == len(tests), "Native selectors must be unique"
    expected = set(BUS + WEBKIT + FUNCTIONAL + VISUAL)
    assert set(result) == expected, f"Native selector registration mismatch: {set(result) ^ expected}"
    return result


def run_group(args):
    binary = str(Path(args.binary).resolve())
    tests = registered(binary)
    root = Path(args.output).resolve() / f"scale-{args.scale}"
    root.mkdir(parents=True, exist_ok=True)
    for selector in GROUPS[args.group]:
        output = root / selector
        output.mkdir(parents=True, exist_ok=True)
        env = dict(os.environ, GTK_A11Y="none", GDK_SCALE=str(args.scale),
                   GDK_DPI_SCALE="1", KARIN_TEST_SECRET_BUS="1",
                   KARIN_V04_ASSERT_DENSITY="1", KARIN_TEST_CAPTURE_DIR=str(output))
        visual = args.group in {"visual", "lyrics", "high-contrast"}
        env["GSK_RENDERER"] = "ngl" if visual else "cairo"
        env["GSK_DEBUG"] = "full-redraw"
        if visual or selector in {
            "native_bundled_typography",
            "native_bundled_symbolic_icons_recolor",
            "native_update_dialog_adaptive_status_and_notes",
            "native_collection_filter_continuation_and_exact_occurrences",
            "native_music_shuffle_repeat_mpris_shared_policy_and_eos",
            "native_audio_cache_application_process_restart",
            "native_global_search_measured_slot_and_motion",
            "native_music_player_progress_duration_transitions",
        } or selector.startswith(("native_v07_", "native_v08_", "native_music_resume_")):
            env["G_DEBUG"] = "fatal-warnings"
        if args.group == "high-contrast":
            env["ADW_DEBUG_HIGH_CONTRAST"] = "1"
        if selector in {VISUAL[0], VISUAL[2]} or args.group == "lyrics":
            env.pop("GSK_DEBUG", None)  # Ordinary damage tracking for motion.
        if selector.startswith(("native_v07_", "native_v08_", "native_music_resume_")) or selector in {
            "native_audio_cache_application_process_restart",
            "native_global_search_measured_slot_and_motion",
        }:
            env.pop("GSK_DEBUG", None)
        cold_profile = None
        if selector == VISUAL[0]:
            # Cold observations cannot inherit the static/motion profile.
            cold_profile = tempfile.TemporaryDirectory(prefix="karin-ci-cold-")
            profile = Path(cold_profile.name)
            for name in ("config", "cache", "data", "state"):
                (profile / name).mkdir(parents=True)
                env[f"XDG_{name.upper()}_HOME"] = str(profile / name)
        command = [binary, tests[selector], "--exact", "--ignored", "--test-threads=1", "--nocapture"]
        if selector not in BUS:
            width, height = (2800, 1600) if selector == "native_global_search_measured_slot_and_motion" else (1920, 1080)
            screen = f"{width * args.scale}x{height * args.scale}x24"
            command = ["xvfb-run", "-a", "-s", f"-screen 0 {screen} -dpi 96", *command]
            if selector in WEBKIT[1:]:
                # These two tests need a WM before real mouse/keyboard input.
                session = ('openbox --sm-disable >/tmp/karin-ci-webkit-openbox.log 2>&1 &\n'
                           'wm_pid=$!\ntrap \'kill "$wm_pid" 2>/dev/null || true; wait "$wm_pid" 2>/dev/null || true\' EXIT\n'
                           'attempt=0\nuntil wmctrl -m >/dev/null 2>&1; do\n'
                           '  attempt=$((attempt + 1)); test "$attempt" -lt 50; sleep 0.1\ndone\n"$@"')
                command = [*command[:4], "bash", "-euc", session, "webkit-session", *command[4:]]
        command = ["dbus-run-session", "--", *command]
        print(f"::group::{selector} (scale {args.scale})", flush=True)
        start = time.monotonic()
        with (output / "test.log").open("w") as log:
            process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
            for line in process.stdout:
                print(line, end="", flush=True)
                log.write(line)
            code = process.wait()
        if cold_profile:
            cold_profile.cleanup()
        (output / "result.json").write_text(json.dumps({"selector": selector, "scale": args.scale,
            "elapsed_seconds": time.monotonic() - start, "exit_code": code}) + "\n")
        print("::endgroup::", flush=True)
        if code:
            raise SystemExit(f"Native fixture failed ({code}): {selector}; see {output / 'test.log'}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    artifact = commands.add_parser("artifact", help="Select the single Cargo all-features library test ELF")
    artifact.add_argument("jsonl")
    verify = commands.add_parser("verify", help="Fail if any ignored fixture is missing or unregistered")
    verify.add_argument("binary")
    run = commands.add_parser("run")
    run.add_argument("group", choices=GROUPS)
    run.add_argument("binary")
    run.add_argument("--scale", type=int, choices=(1, 2), default=1)
    run.add_argument("--output", default="build/native-tests")
    args = parser.parse_args()
    if args.command == "artifact":
        artifacts = [json.loads(line) for line in Path(args.jsonl).read_text().splitlines()]
        binaries = {item["executable"] for item in artifacts
                    if item.get("reason") == "compiler-artifact"
                    and item.get("target", {}).get("name") == "karin"
                    and item.get("target", {}).get("kind") == ["lib"]
                    and item.get("profile", {}).get("test") and item.get("executable")}
        assert len(binaries) == 1, f"Expected one library test executable: {binaries}"
        binary = Path(binaries.pop()).resolve()
        assert binary.is_file() and os.access(binary, os.X_OK), f"Test executable unavailable: {binary}"
        print(binary)
    elif args.command == "verify":
        tests = registered(args.binary)
        print(f"Registered {len(tests)} ignored fixtures: {len(FUNCTIONAL)} functional, {len(BUS)} D-Bus, {len(WEBKIT)} WebKit, {len(VISUAL)} visual")
    else:
        run_group(args)


if __name__ == "__main__":
    main()
