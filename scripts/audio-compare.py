#!/usr/bin/env python3
"""
Compares two float32 stereo WAV files (active vs shadow recorder outputs).

Usage:
    audio-compare.py ACTIVE.wav SHADOW.wav [--floor RMS] [--block-ms 100] [--csv out.csv]

Exit codes:
    0: Success (files match within thresholds or are identical)
    1: Error (energy floor check failed)
    2: Error (file format mismatch or length mismatch)
"""

import sys
import struct
import numpy as np
import argparse


def parse_wav(filepath):
    """
    Manual RIFF parser for WAV files containing float32 PCM data.
    Returns the numpy array of samples.
    """
    try:
        with open(filepath, 'rb') as f:
            data = f.read()
    except FileNotFoundError:
        print(f"Error: File not found: {filepath}", file=sys.stderr)
        sys.exit(2)

    if len(data) < 44:
        print(f"Error: File too small to be valid WAV: {filepath}", file=sys.stderr)
        sys.exit(2)

    # Check RIFF header
    if data[:4] != b'RIFF' or data[8:12] != b'WAVE':
        print(f"Error: Invalid WAV header: {filepath}", file=sys.stderr)
        sys.exit(2)

    # Find fmt and data chunks
    offset = 12
    fmt_chunk = None
    data_chunk = None

    while offset < len(data):
        if offset + 8 > len(data):
            break
        chunk_id = data[offset:offset+4]
        chunk_size = struct.unpack('<I', data[offset+4:offset+8])[0]
        
        actual_size = chunk_size
        
        if chunk_id == b'fmt ':
            if offset + 8 + actual_size > len(data):
                print(f"Error: Truncated fmt chunk in {filepath}", file=sys.stderr)
                sys.exit(2)
            fmt_chunk = data[offset+8:offset+8+actual_size]
        elif chunk_id == b'data':
            if offset + 8 + actual_size > len(data):
                print(f"Error: Truncated data chunk in {filepath}", file=sys.stderr)
                sys.exit(2)
            data_chunk = data[offset+8:offset+8+actual_size]
        
        offset += 8 + actual_size + (actual_size & 1)  # RIFF chunks are padded to even sizes

    if fmt_chunk is None:
        print(f"Error: No fmt chunk found in {filepath}", file=sys.stderr)
        sys.exit(2)
    if data_chunk is None:
        print(f"Error: No data chunk found in {filepath}", file=sys.stderr)
        sys.exit(2)

    # Parse fmt chunk
    if len(fmt_chunk) < 16:
        print(f"Error: fmt chunk too small in {filepath}", file=sys.stderr)
        sys.exit(2)
    
    audio_format = struct.unpack('<H', fmt_chunk[0:2])[0]
    num_channels = struct.unpack('<H', fmt_chunk[2:4])[0]
    sample_rate = struct.unpack('<I', fmt_chunk[4:8])[0]
    bits_per_sample = struct.unpack('<H', fmt_chunk[14:16])[0]  # fmt: format, channels, rate, byte rate, align, bits

    # Validate format
    if audio_format != 3: # IEEE Float
        print(f"Error: Unsupported audio format {audio_format} (expected 3 for float) in {filepath}", file=sys.stderr)
        sys.exit(2)
    
    if num_channels != 2:
        print(f"Error: Expected 2 channels, got {num_channels} in {filepath}", file=sys.stderr)
        sys.exit(2)
    
    if bits_per_sample != 32:
        print(f"Error: Expected 32 bits per sample, got {bits_per_sample} in {filepath}", file=sys.stderr)
        sys.exit(2)

    # Parse data
    if len(data_chunk) % 8 != 0: # 2 channels * 4 bytes float
        print(f"Error: Data chunk size not aligned to 8 bytes in {filepath}", file=sys.stderr)
        sys.exit(2)

    samples = np.frombuffer(data_chunk, dtype=np.float32)
    return samples, sample_rate


def compute_rms_dbfs(samples):
    """
    Compute RMS level in dBFS for a stereo sample array (N x 2).
    Returns scalar dBFS value.
    """
    # RMS over all samples
    rms = np.sqrt(np.mean(samples ** 2))
    if rms == 0:
        return -np.inf
    return 20 * np.log10(rms)


def main():
    parser = argparse.ArgumentParser(description='Compare two stereo float32 WAV files.')
    parser.add_argument('active_file', help='Path to the active recorder WAV file')
    parser.add_argument('shadow_file', help='Path to the shadow recorder WAV file')
    parser.add_argument('--floor', type=float, default=1e-4, help='Energy floor for RMS (default: 1e-4)')
    parser.add_argument('--block-ms', type=int, default=100, help='Block size in milliseconds (default: 100)')
    parser.add_argument('--csv', type=str, default=None, help='Output per-block table to CSV')
    
    args = parser.parse_args()

    # Parse WAV files
    active_samples, active_sr = parse_wav(args.active_file)
    shadow_samples, shadow_sr = parse_wav(args.shadow_file)

    # Validate sample rates match
    if active_sr != shadow_sr:
        print(f"Error: Sample rate mismatch: {active_sr} vs {shadow_sr}", file=sys.stderr)
        sys.exit(2)

    # Validate lengths match
    if len(active_samples) != len(shadow_samples):
        print(f"Error: File length mismatch: {len(active_samples)} vs {len(shadow_samples)} samples", file=sys.stderr)
        sys.exit(2)

    total_samples = len(active_samples)
    if total_samples == 0:
        print("Error: Empty audio files", file=sys.stderr)
        sys.exit(2)

    # Check for identical files
    if np.array_equal(active_samples, shadow_samples):
        print("identical")
        sys.exit(0)

    sr = active_sr
    # the data are interleaved stereo: a block of block_ms holds two samples per frame
    block_samples = 2 * int(sr * args.block_ms / 1000.0)
    if block_samples <= 0:
        print("Error: Block size results in 0 samples", file=sys.stderr)
        sys.exit(2)

    # Reshape samples to (num_blocks, block_samples, 2) if possible, else handle remainder
    # We assume the recorder produces consistent blocks, but we handle arbitrary length by padding or truncating?
    # Spec says "same run, same length", so we assume lengths are multiples of block_samples ideally, 
    # or we process full blocks and ignore remainder? 
    # Let's process full blocks only to keep it clean, or pad with zeros?
    # Standard practice: process full blocks. If remainder exists, it might be considered part of the last block or ignored.
    # Given "same length", let's assume length is a multiple of block_samples for simplicity, 
    # or we just iterate through full blocks.
    
    num_full_blocks = total_samples // block_samples
    
    if num_full_blocks == 0:
        # Files are shorter than one block
        # Compute overall stats as a single block?
        # Let's treat the whole file as one block for analysis if it's short?
        # Or fail? Spec doesn't specify. Let's process what we have.
        # If total_samples < block_samples, we have 1 block.
        num_full_blocks = 1
        # We will use the actual samples for this block, padding if necessary? 
        # No, just use the samples.
    
    # Prepare arrays for results
    # We need to reshape into (num_blocks, block_samples, 2)
    # If total_samples is not a multiple, we might have issues.
    # Let's assume the input is well-formed and length is a multiple of block_samples.
    # If not, we'll take the first num_full_blocks * block_samples samples.
    
    valid_len = num_full_blocks * block_samples
    active_trimmed = active_samples[:valid_len]
    shadow_trimmed = shadow_samples[:valid_len]
    
    # Reshape to (num_blocks, block_samples, 2)
    # Flatten to (num_blocks * block_samples,), then reshape
    active_blocks = active_trimmed.reshape(num_full_blocks, block_samples)
    shadow_blocks = shadow_trimmed.reshape(num_full_blocks, block_samples)
    
    # If there's a remainder, we ignore it for block-wise comparison, 
    # but we should probably include it in the energy floor check?
    # Let's check energy on the WHOLE file first.
    
    # Energy Floor Check
    active_rms_overall = np.sqrt(np.mean(active_samples ** 2))
    shadow_rms_overall = np.sqrt(np.mean(shadow_samples ** 2))
    
    if active_rms_overall < args.floor or shadow_rms_overall < args.floor:
        print("FAIL energy floor")
        sys.exit(1)

    # Per-block analysis
    block_results = []
    silent_count = 0
    relative_diffs = []
    worst_5 = []
    
    for i in range(num_full_blocks):
        a_block = active_blocks[i]
        s_block = shadow_blocks[i]
        
        # RMS for each side (over both channels)
        # RMS is sqrt(mean(x^2))
        a_rms = np.sqrt(np.mean(a_block ** 2))
        s_rms = np.sqrt(np.mean(s_block ** 2))
        
        # dBFS
        if a_rms == 0:
            a_dbfs = -np.inf
        else:
            a_dbfs = 20 * np.log10(a_rms)
            
        if s_rms == 0:
            s_dbfs = -np.inf
        else:
            s_dbfs = 20 * np.log10(s_rms)
            
        # Difference RMS (dBFS) - usually |A - S| in dB? 
        # Spec says "difference RMS (dBFS)". 
        # Let's compute the RMS of the difference signal.
        diff_block = a_block - s_block
        diff_rms = np.sqrt(np.mean(diff_block ** 2))
        if diff_rms == 0:
            diff_dbfs = -np.inf
        else:
            diff_dbfs = 20 * np.log10(diff_rms)
            
        # Difference relative to the louder side (dB)
        # max(a_rms, s_rms)
        louder_rms = max(a_rms, s_rms)
        if louder_rms == 0:
            relative_diff_db = 0.0 # Both silent
        else:
            with np.errstate(divide='ignore', invalid='ignore'):  # silent blocks are excluded below
                relative_diff_db = 20 * np.log10(diff_rms / louder_rms)
            
        # Peak |diff|
        peak_diff = np.max(np.abs(diff_block))
        
        # Time of block
        block_time_s = i * args.block_ms / 1000.0
        
        # Silence check: both sides < -90 dBFS
        # -inf < -90 is True
        is_silent = (a_dbfs < -90) and (s_dbfs < -90)
        if is_silent:
            silent_count += 1
        else:
            relative_diffs.append(relative_diff_db)
            
        block_results.append({
            'block': i,
            'time_s': block_time_s,
            'active_dbfs': a_dbfs,
            'shadow_dbfs': s_dbfs,
            'diff_dbfs': diff_dbfs,
            'relative_diff_db': relative_diff_db,
            'peak_diff': peak_diff
        })
        
        block_results[-1]['silent'] = is_silent

    # Summary Statistics
    num_blocks = num_full_blocks
    
    # Median and 95th percentile of relative difference over non-silent blocks
    if len(relative_diffs) > 0:
        median_rel_diff = np.median(relative_diffs)
        p95_rel_diff = np.percentile(relative_diffs, 95)
    else:
        median_rel_diff = 0.0
        p95_rel_diff = 0.0
        
    # Worst 5 blocks
    # Sort by relative_diff_db descending
    sorted_blocks = sorted((b for b in block_results if not b['silent']), key=lambda x: x['relative_diff_db'], reverse=True)
    worst_5_blocks = sorted_blocks[:5]
    
    # Print Summary
    print(f"Blocks: {num_blocks}")
    print(f"Silent blocks (< -90 dBFS): {silent_count}")
    print(f"Median relative difference (dB): {median_rel_diff:.2f}")
    print(f"95th percentile relative difference (dB): {p95_rel_diff:.2f}")
    print("Worst 5 blocks:")
    for b in worst_5_blocks:
        print(f"  Block {b['block']} (t={b['time_s']:.3f}s): rel_diff={b['relative_diff_db']:.2f} dB, peak_diff={b['peak_diff']:.6f}")

    # CSV Output
    if args.csv:
        with open(args.csv, 'w', newline='') as f:
            writer = None
            # Write header
            f.write("block,time_s,active_dbfs,shadow_dbfs,diff_dbfs,relative_diff_db,peak_diff\n")
            for b in block_results:
                f.write(f"{b['block']},{b['time_s']:.3f},{b['active_dbfs']:.2f},{b['shadow_dbfs']:.2f},{b['diff_dbfs']:.2f},{b['relative_diff_db']:.2f},{b['peak_diff']:.6f}\n")

    sys.exit(0)


if __name__ == '__main__':
    main()
