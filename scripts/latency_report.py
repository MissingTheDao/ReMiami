#!/usr/bin/env python3
"""
Latency report generator for retro-audio / reMiami audio latency measurements.

Reads probe.csv, meta.txt, and optionally mon.wav from one or more RUN_DIRs,
computes latency metrics per the LATENCY_METHOD.md spec, and prints a Markdown table.

Uses only the Python 3 standard library.
"""

import argparse
import array
import csv
import math
import os
import statistics
import sys
import tempfile
import wave as wave_mod


def parse_meta(path: str) -> dict:
    """Parse meta.txt into a dict of key=value pairs."""
    meta = {}
    if not os.path.exists(path):
        return meta
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if "=" in line:
                key, _, val = line.partition("=")
                meta[key.strip()] = val.strip()
    return meta


def parse_probe_csv(path: str):
    """
    Parse probe.csv.
    Expected format:
      Header: probelog,1
      Event lines: event,<index>,<t_ns>
      Callback lines: cb,<t_in>,<t_out>,<queued_before>,<frames>,<onset>,<total>
    Returns (events_list, callbacks_list).
    events_list: list of (index, t_ns)
    callbacks_list: list of (t_in, t_out, queued_before, frames, onset, total)
    """
    events = []
    callbacks = []
    if not os.path.exists(path):
        return events, callbacks

    with open(path, "r", encoding="utf-8") as f:
        reader = csv.reader(f)
        header = next(reader, None)
        if not header or header[0] != "probelog" or header[1] != "1":
            # Try to read anyway, maybe header is different
            pass
        
        for row in reader:
            if not row or row[0] == "probelog":
                continue
            tag = row[0]
            if tag == "event":
                # event,<index>,<t_ns>
                if len(row) >= 3:
                    try:
                        idx = int(row[1])
                        t_ns = int(row[2])
                        events.append((idx, t_ns))
                    except ValueError:
                        pass
            elif tag == "cb":
                # cb,<t_in>,<t_out>,<queued_before>,<frames>,<onset>,<total>
                if len(row) >= 7:
                    try:
                        t_in = int(row[1])
                        t_out = int(row[2])
                        queued_before = int(row[3])
                        frames = int(row[4])
                        onset = int(row[5])
                        total = int(row[6])
                        callbacks.append((t_in, t_out, queued_before, frames, onset, total))
                    except ValueError:
                        pass
    return events, callbacks


def parse_mon_wav(path: str):
    """
    Parse mon.wav (stereo or mono, 48000 Hz).
    Supports fmt tag 1 (PCM) and 3 (IEEE float).
    Returns list of left-channel float samples.
    """
    if not os.path.exists(path):
        return None
    
    samples = []
    try:
        with wave_mod.open(path, "rb") as wf:
            n_channels = wf.getnchannels()
            sampwidth = wf.getsampwidth()
            rate = wf.getframerate()
            n_frames = wf.getnframes()
            comp_type = wf.getcomptype()
            
            # We expect 48000 Hz, but proceed regardless
            raw_data = wf.readframes(n_frames)
            
            num_samples = n_frames * n_channels
            
            if sampwidth == 2:
                # 16-bit PCM
                import struct
                fmt = "<" + "h" * num_samples
                data = struct.unpack(fmt, raw_data)
                samples = [v / 32768.0 for v in data]
            elif sampwidth == 4:
                # 32-bit
                if comp_type == b'NONE':
                    # Assume float
                    import struct
                    fmt = "<" + "f" * num_samples
                    data = struct.unpack(fmt, raw_data)
                    samples = list(data)
                else:
                    # Assume PCM
                    import struct
                    fmt = "<" + "i" * num_samples
                    data = struct.unpack(fmt, raw_data)
                    samples = [v / 2147483648.0 for v in data]
            else:
                return None
            
            # Extract left channel if stereo
            if n_channels == 2:
                samples = samples[0::2]
            
            return samples
    except Exception:
        return None


def find_onsets_in_wav(samples, sample_rate=48000):
    """
    Find impulse onsets in wav samples.
    Onset: first sample index where |left| > 0.25 after at least 100 ms (4800 samples) below 0.25.
    """
    if samples is None:
        return []
    
    quiet_threshold = 0.25
    quiet_duration_samples = int(0.1 * sample_rate)  # 4800 samples
    
    onsets = []
    in_quiet = True
    quiet_count = 0
    
    for i, s in enumerate(samples):
        if abs(s) < quiet_threshold:
            quiet_count += 1
            if quiet_count >= quiet_duration_samples:
                in_quiet = True
        else:
            if in_quiet and quiet_count >= quiet_duration_samples:
                # Onset detected at previous sample (the last quiet one) + 1
                # Actually, the onset is the current sample where it exceeds threshold
                # But we need to ensure we had 100ms of quiet before.
                # The onset is the first sample > threshold after quiet period.
                onsets.append(i)
                in_quiet = False
                quiet_count = 0
            elif not in_quiet:
                # Brief dip below threshold, reset quiet count
                quiet_count = 0
                # If it goes back below, we might restart quiet counting
                if abs(s) < quiet_threshold:
                    quiet_count = 1
                    if quiet_count >= quiet_duration_samples:
                        in_quiet = True
    
    return onsets


def percentile_nearest_rank(sorted_list, p):
    """
    Compute percentile using nearest-rank method.
    p is between 0 and 100.
    """
    if not sorted_list:
        return None
    n = len(sorted_list)
    k = math.ceil(p / 100.0 * n)
    k = max(1, min(k, n))
    return sorted_list[k - 1]


def compute_lsw(events, callbacks, group_delay_frames=0):
    """
    Compute L_sw per probe event.
    L_sw = (t_cb_in - t_event)/1e6 + (queued_before + onset + group_delay_frames)/48000 * 1000 ms
    Lost if no callback within 200 ms after event.
    """
    lsw_values = []
    lost_count = 0
    
    # Sort callbacks by t_in for efficient searching
    sorted_callbacks = sorted(callbacks, key=lambda c: c[0])
    
    for idx, t_event in events:
        # Find first callback with t_in >= t_event and onset >= 0
        found = False
        for c in sorted_callbacks:
            t_in, t_out, queued_before, frames, onset, total = c
            if t_in >= t_event and onset >= 0:
                # Check if within 200 ms
                delta_ms = (t_in - t_event) / 1e6
                if delta_ms > 200:
                    lost_count += 1
                    found = False
                    break
                
                # Compute L_sw
                l_sw_ms = delta_ms + (queued_before + onset + group_delay_frames) / 48000.0 * 1000.0
                lsw_values.append(l_sw_ms)
                found = True
                break
        
        if not found:
            lost_count += 1
    
    return lsw_values, lost_count


def compute_cb_stats(callbacks):
    """
    Compute C_cb stats: median, p99, max in microseconds.
    C_cb = t_out - t_in
    """
    cb_durations_us = []
    for c in callbacks:
        t_in, t_out, _, _, _, _ = c
        duration_us = (t_out - t_in)
        cb_durations_us.append(duration_us)
    
    if not cb_durations_us:
        return (None, None, None)
    
    sorted_durations = sorted(cb_durations_us)
    med = statistics.median(sorted_durations)
    p99 = percentile_nearest_rank(sorted_durations, 99)
    max_val = max(sorted_durations)
    
    return (med, p99, max_val)


def compute_jitter(events, mon_samples, sample_rate=48000):
    """
    Compute jitter J from monitor wav.
    d_i = (m_i - m_0)/sample_rate * 1000 - (t_e_i - t_e_0)/1e6 ms
    J sd = pstdev(d), spread = p95 - p5, max |d|
    """
    if mon_samples is None or len(events) < 2:
        return None, None, None
    
    onsets = find_onsets_in_wav(mon_samples, sample_rate)
    
    if len(onsets) < 2:
        return None, None, None
    
    # Pair onsets with events in order
    n_pairs = min(len(onsets), len(events))
    onsets = onsets[:n_pairs]
    event_times = [e[1] for e in events[:n_pairs]]
    
    # Convert to ms
    t0_e = event_times[0]
    t0_m = onsets[0] / sample_rate * 1000.0
    
    d_values = []
    for i in range(n_pairs):
        t_e_i = event_times[i]
        t_m_i = onsets[i] / sample_rate * 1000.0
        
        # d_i = (m_i - m_0) - (t_e_i - t_e_0) in ms
        d_i = (t_m_i - t0_m) - (t_e_i - t0_e) / 1e6
        d_values.append(d_i)
    
    if not d_values:
        return None, None, None
    
    sd = statistics.pstdev(d_values)
    sorted_d = sorted(d_values)
    p5 = percentile_nearest_rank(sorted_d, 5)
    p95 = percentile_nearest_rank(sorted_d, 95)
    spread = p95 - p5
    max_abs_d = max(abs(d) for d in d_values)
    
    return (sd, spread, max_abs_d)


def format_ms(val):
    """Format ms value with 2 decimals."""
    if val is None:
        return "n/a"
    return f"{val:.2f}"


def format_us(val):
    """Format µs value as integer."""
    if val is None:
        return "n/a"
    return f"{int(val)}"


def format_frames(val):
    """Format frames as integer."""
    if val is None:
        return "n/a"
    return f"{int(val)}"


def process_run(run_dir):
    """Process a single RUN_DIR and return a row dict."""
    meta_path = os.path.join(run_dir, "meta.txt")
    probe_path = os.path.join(run_dir, "probe.csv")
    mon_path = os.path.join(run_dir, "mon.wav")
    
    meta = parse_meta(meta_path)
    events, callbacks = parse_probe_csv(probe_path)
    
    # Extract meta fields
    path = meta.get("path", "unknown")
    load = meta.get("load", "unknown")
    hint = meta.get("hint", "unset")
    granted_frames = meta.get("granted_frames", "n/a")
    quant = meta.get("quant", None)
    rate = meta.get("rate", None)
    sink_latency_us = meta.get("sink_latency_us", "0")
    lmix_frames = meta.get("lmix_frames", "0")
    group_delay_frames = meta.get("group_delay_frames", "0")
    xruns = meta.get("xruns", "0")
    
    # Parse numeric fields
    try:
        granted_frames_int = int(granted_frames) if granted_frames != "n/a" else None
    except ValueError:
        granted_frames_int = None
    
    try:
        sink_latency_us_float = float(sink_latency_us)
    except ValueError:
        sink_latency_us_float = 0.0
    
    try:
        lmix_frames_int = int(lmix_frames)
    except ValueError:
        lmix_frames_int = 0
    
    try:
        group_delay_frames_int = int(group_delay_frames)
    except ValueError:
        group_delay_frames_int = 0
    
    try:
        xruns_int = int(xruns)
    except ValueError:
        xruns_int = 0
    
    # QUANT/RATE string
    quant_rate_str = "n/a"
    if quant is not None and rate is not None:
        try:
            quant_int = int(quant)
            rate_int = int(rate)
            quant_rate_str = f"{quant_int}/{rate_int}"
        except ValueError:
            quant_rate_str = "n/a"
    
    # L_dev ms
    l_dev_ms = None
    if quant_rate_str != "n/a":
        try:
            quant_int = int(quant)
            rate_int = int(rate)
            l_dev_ms = quant_int / rate_int * 1000.0 + sink_latency_us_float / 1000.0
        except (ValueError, ZeroDivisionError):
            l_dev_ms = None
    
    # L_sw
    lsw_values, lost_count = compute_lsw(events, callbacks, group_delay_frames_int)
    
    # C_cb stats
    cb_med, cb_p99, cb_max = compute_cb_stats(callbacks)
    
    # L_total median
    l_total_med = None
    if lsw_values:
        lsw_med = statistics.median(lsw_values)
        if l_dev_ms is not None:
            l_total_med = lsw_med + l_dev_ms
    
    # Jitter
    mon_samples = parse_mon_wav(mon_path)
    j_sd, j_spread, j_max = compute_jitter(events, mon_samples)
    
    # Format L_sw stats
    lsw_med_str = format_ms(statistics.median(lsw_values)) if lsw_values else "n/a"
    lsw_p95_str = format_ms(percentile_nearest_rank(sorted(lsw_values), 95)) if lsw_values else "n/a"
    lsw_max_str = format_ms(max(lsw_values)) if lsw_values else "n/a"
    
    # Build row
    row = {
        "path": path,
        "load": load,
        "hint": hint,
        "granted_frames": format_frames(granted_frames_int) if granted_frames_int is not None else "n/a",
        "quant_rate": quant_rate_str,
        "sink_lat_ms": format_ms(sink_latency_us_float / 1000.0) if sink_latency_us_float else "n/a",
        "l_mix_fr": format_frames(lmix_frames_int),
        "l_sw_med": lsw_med_str,
        "l_sw_p95": lsw_p95_str,
        "l_sw_max": lsw_max_str,
        "l_dev_ms": format_ms(l_dev_ms) if l_dev_ms is not None else "n/a",
        "l_total_med": format_ms(l_total_med) if l_total_med is not None else "n/a",
        "j_sd": format_ms(j_sd) if j_sd is not None else "n/a",
        "j_spread": format_ms(j_spread) if j_spread is not None else "n/a",
        "c_cb_med": format_us(cb_med),
        "c_cb_p99": format_us(cb_p99),
        "c_cb_max": format_us(cb_max),
        "lost": str(lost_count),
        "xruns": str(xruns_int),
    }
    
    return row


def print_table(rows):
    """Print Markdown table."""
    headers = [
        "path",
        "load",
        "hint",
        "granted_frames",
        "quant_rate",
        "sink_lat_ms",
        "l_mix_fr",
        "l_sw_med",
        "l_sw_p95",
        "l_sw_max",
        "l_dev_ms",
        "l_total_med",
        "j_sd",
        "j_spread",
        "c_cb_med",
        "c_cb_p99",
        "c_cb_max",
        "lost",
        "xruns",
    ]
    
    # Print header
    header_line = "| " + " | ".join(headers) + " |"
    sep_line = "|" + "|".join(["---" for _ in headers]) + "|"
    print(header_line)
    print(sep_line)
    
    # Print rows
    for row in rows:
        vals = [row.get(h, "") for h in headers]
        print("| " + " | ".join(vals) + " |")


def generate_selftest_data(run_dir):
    """
    Generate synthetic RUN_DIR for selftest.
    Events every 250 ms, callbacks every 10 ms with known queued/onset so L_sw = 15.0 ms exactly.
    mon.wav with impulses offset consistently.
    """
    os.makedirs(run_dir, exist_ok=True)
    
    # Generate meta.txt
    meta_content = """path=retrio
load=idle
hint=512
granted_frames=512
quant=4096
rate=48000
sink_latency_us=1000
lmix_frames=0
group_delay_frames=0
xruns=0
"""
    with open(os.path.join(run_dir, "meta.txt"), "w") as f:
        f.write(meta_content)
    
    # Generate probe.csv
    # Events every 250 ms starting at t=0
    # Callbacks every 10 ms
    # L_sw = (t_cb_in - t_event)/1e6 + (queued_before + onset + group_delay_frames)/48000 * 1000
    # We want L_sw = 15.0 ms exactly.
    # Let's set:
    #   t_cb_in - t_event = 10 ms (1 callback delay)
    #   queued_before = 0
    #   onset = 0
    #   group_delay_frames = 0
    #   L_sw = 10 ms + 0 = 10 ms. Not 15.
    # Let's adjust:
    #   t_cb_in - t_event = 5 ms
    #   queued_before = 120 frames (120/48000 * 1000 = 2.5 ms)
    #   onset = 0
    #   L_sw = 5 + 2.5 = 7.5 ms. Not 15.
    # Let's try:
    #   t_cb_in - t_event = 10 ms
    #   queued_before = 240 frames (240/48000 * 1000 = 5 ms)
    #   onset = 0
    #   L_sw = 10 + 5 = 15 ms. Perfect.
    
    base_time = 1000000000  # 1 second in ns, arbitrary base
    events = []
    callbacks = []
    
    num_events = 20
    event_interval_ns = 250_000_000  # 250 ms
    
    for i in range(num_events):
        t_event = base_time + i * event_interval_ns
        events.append((i, t_event))
        
        # Callback 10 ms after event
        t_cb_in = t_event + 10_000_000  # 10 ms
        t_cb_out = t_cb_in + 100_000  # 100 µs processing time
        queued_before = 240
        frames = 512
        onset = 0
        total = i * 512
        
        callbacks.append((t_cb_in, t_cb_out, queued_before, frames, onset, total))
    
    # Write probe.csv
    with open(os.path.join(run_dir, "probe.csv"), "w") as f:
        f.write("probelog,1\n")
        for idx, t_ns in events:
            f.write(f"event,{idx},{t_ns}\n")
        for t_in, t_out, queued_before, frames, onset, total in callbacks:
            f.write(f"cb,{t_in},{t_out},{queued_before},{frames},{onset},{total}\n")
    
    # Generate mon.wav
    # Impulses at the same time as events, but with consistent offset.
    # We'll create a stereo wav with impulses at the expected onset times.
    # Onset time in samples: t_event + 10 ms + 240 frames = t_event + 10 ms + 5 ms = t_event + 15 ms
    # So the impulse in mon.wav should be at t_event + 15 ms.
    
    sample_rate = 48000
    duration = num_events * event_interval_ns / 1e9 + 0.5  # extra 0.5 s
    n_samples = int(duration * sample_rate)
    
    samples = [0.0] * n_samples
    
    for i in range(num_events):
        t_event_ns = base_time + i * event_interval_ns
        t_event_s = t_event_ns / 1e9
        onset_s = t_event_s + 0.015  # 15 ms offset
        onset_sample = int(onset_s * sample_rate)
        
        if 0 <= onset_sample < n_samples:
            samples[onset_sample] = 0.9  # (one frame; written to both channels)
    
    # Write wav
    with wave_mod.open(os.path.join(run_dir, "mon.wav"), "wb") as wf:
        wf.setnchannels(2)
        wf.setsampwidth(2)  # 16-bit
        wf.setframerate(sample_rate)
        
        # Pack samples as 16-bit PCM
        # interleaved stereo: the same value on both channels per frame
        pcm = array.array("h", (max(-32768, min(32767, int(v * 32767))) for v in samples for _ in range(2)))
        if sys.byteorder == "big":
            pcm.byteswap()
        wf.writeframes(pcm.tobytes())


def run_selftest():
    """Run selftest with synthetic data."""
    with tempfile.TemporaryDirectory() as tmpdir:
        run_dir = os.path.join(tmpdir, "selftest")
        generate_selftest_data(run_dir)
        
        rows = [process_run(run_dir)]
        
        # Assert L_sw median == 15.0
        lsw_med_str = rows[0]["l_sw_med"]
        lsw_med = float(lsw_med_str)
        assert abs(lsw_med - 15.0) < 0.01, f"L_sw median {lsw_med} != 15.0"
        
        # Assert J sd < 0.05
        j_sd_str = rows[0]["j_sd"]
        if j_sd_str != "n/a":
            j_sd = float(j_sd_str)
            assert j_sd < 0.05, f"J sd {j_sd} >= 0.05"
        
        print("selftest ok")


def main():
    parser = argparse.ArgumentParser(description="Latency report generator")
    parser.add_argument("run_dirs", nargs="*", help="Directories containing probe.csv, meta.txt, and optionally mon.wav")
    parser.add_argument("--selftest", action="store_true", help="Run selftest with synthetic data")
    
    args = parser.parse_args()
    
    if args.selftest:
        run_selftest()
        return
    
    rows = []
    for run_dir in args.run_dirs:
        if not os.path.isdir(run_dir):
            print(f"Warning: {run_dir} is not a directory, skipping.", file=sys.stderr)
            continue
        
        if not os.path.exists(os.path.join(run_dir, "probe.csv")):
            print(f"Warning: {run_dir} does not contain probe.csv, skipping.", file=sys.stderr)
            continue
        
        row = process_run(run_dir)
        rows.append(row)
    
    if rows:
        print_table(rows)
    else:
        print("No valid runs found.", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
