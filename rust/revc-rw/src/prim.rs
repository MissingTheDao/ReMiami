//! Index and mip helpers for the immediate-mode renderer (drafted by the local model, reviewed).

pub fn expand_primitive(prim: u32, indices: &[u32], out: &mut Vec<u32>) {
    let n = indices.len();
    match prim {
        1 => {
            // Line list: pairs, ignore trailing odd index
            let count = (n / 2) * 2;
            for i in (0..count).step_by(2) {
                out.push(indices[i]);
                out.push(indices[i + 1]);
            }
        }
        2 => {
            // Polyline / line strip: for i in 0..n-1 push idx[i] and idx[i+1]
            if n >= 2 {
                for i in 0..n - 1 {
                    out.push(indices[i]);
                    out.push(indices[i + 1]);
                }
            }
        }
        3 => {
            // Triangle list: copy whole triples, ignore leftovers
            let count = (n / 3) * 3;
            for i in (0..count).step_by(3) {
                out.push(indices[i]);
                out.push(indices[i + 1]);
                out.push(indices[i + 2]);
            }
        }
        4 => {
            // Triangle strip: for i in 0..n-2
            if n >= 3 {
                for i in 0..n - 2 {
                    if i % 2 == 0 {
                        out.push(indices[i]);
                        out.push(indices[i + 1]);
                        out.push(indices[i + 2]);
                    } else {
                        out.push(indices[i + 1]);
                        out.push(indices[i]);
                        out.push(indices[i + 2]);
                    }
                }
            }
        }
        5
            // Triangle fan: for i in 1..n-1 push idx[0], idx[i], idx[i+1]
            if n >= 3 => {
                let center = indices[0];
                for i in 1..n - 1 {
                    out.push(center);
                    out.push(indices[i]);
                    out.push(indices[i + 1]);
                }
            }
        _ => {
            // Points or unknown: push nothing
        }
    }
}

pub fn is_line_primitive(prim: u32) -> bool {
    prim == 1 || prim == 2
}

pub fn generate_mips(width: u32, height: u32, base: &[u8]) -> Vec<Vec<u8>> {
    if width == 0 || height == 0 || base.len() != (width * height * 4) as usize {
        return Vec::new();
    }

    let mut levels = Vec::new();
    let mut current_data = base.to_vec();
    let mut w = width;
    let mut h = height;

    loop {
        levels.push(current_data.clone());
        if w <= 1 && h <= 1 {
            break;
        }
        let new_w = if w > 1 { w / 2 } else { 1 };
        let new_h = if h > 1 { h / 2 } else { 1 };
        let new_data = generate_mip_level(&current_data, w, h, new_w, new_h);
        current_data = new_data;
        w = new_w;
        h = new_h;
    }

    levels
}

fn generate_mip_level(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if dst_w == 0 || dst_h == 0 {
        return Vec::new();
    }

    let mut dst = vec![0u8; (dst_w * dst_h * 4) as usize];

    for dy in 0..dst_h {
        for dx in 0..dst_w {
            // Determine the source texels that contribute to this destination texel
            // The source texel at (sx, sy) maps to destination texel at (sx/2, sy/2)
            // So destination (dx, dy) covers source texels in range:
            //   sx in [dx*2, dx*2+1] intersected with [0, src_w)
            //   sy in [dy*2, dy*2+1] intersected with [0, src_h)

            let sx_start = dx * 2;
            let sy_start = dy * 2;
            let sx_end = (dx * 2 + 2).min(src_w);
            let sy_end = (dy * 2 + 2).min(src_h);

            let mut sum_r = 0u32;
            let mut sum_g = 0u32;
            let mut sum_b = 0u32;
            let mut sum_a = 0u32;
            let mut count = 0u32;

            for sy in sy_start..sy_end {
                for sx in sx_start..sx_end {
                    let src_idx = (sy * src_w + sx) * 4;
                    sum_r += src[src_idx as usize] as u32;
                    sum_g += src[src_idx as usize + 1] as u32;
                    sum_b += src[src_idx as usize + 2] as u32;
                    sum_a += src[src_idx as usize + 3] as u32;
                    count += 1;
                }
            }

            let dst_idx = (dy * dst_w + dx) * 4;
            // Every destination texel covers at least one source texel.
            let avg = |sum: u32| ((sum + count / 2) / count.max(1)) as u8;
            dst[dst_idx as usize] = avg(sum_r);
            dst[dst_idx as usize + 1] = avg(sum_g);
            dst[dst_idx as usize + 2] = avg(sum_b);
            dst[dst_idx as usize + 3] = avg(sum_a);
        }
    }

    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_primitive_fan_4_indices() {
        // Triangle fan with 4 indices: indices[0], indices[1], indices[2] and indices[0], indices[2], indices[3]
        let prim = 5u32;
        let indices = vec![0, 1, 2, 3];
        let mut out = Vec::new();
        expand_primitive(prim, &indices, &mut out);
        // Expected: [0,1,2, 0,2,3]
        assert_eq!(out, vec![0, 1, 2, 0, 2, 3]);
    }

    #[test]
    fn test_expand_primitive_strip_5_indices() {
        // Triangle strip with 5 indices
        // i=0 (even): idx[0], idx[1], idx[2]
        // i=1 (odd): idx[2], idx[1], idx[3]
        // i=2 (even): idx[2], idx[3], idx[4]
        let prim = 4u32;
        let indices = vec![0, 1, 2, 3, 4];
        let mut out = Vec::new();
        expand_primitive(prim, &indices, &mut out);
        assert_eq!(out, vec![0, 1, 2, 2, 1, 3, 2, 3, 4]);
    }

    #[test]
    fn test_expand_primitive_polyline_3() {
        // Polyline with 3 indices: (0,1), (1,2)
        let prim = 2u32;
        let indices = vec![0, 1, 2];
        let mut out = Vec::new();
        expand_primitive(prim, &indices, &mut out);
        assert_eq!(out, vec![0, 1, 1, 2]);
    }

    #[test]
    fn test_generate_mips_4x2() {
        // Create a 4x2 RGBA8 image
        let width = 4u32;
        let height = 2u32;
        let mut base = vec![0u8; (width * height * 4) as usize];
        
        // Fill with distinct values for testing
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                base[idx as usize] = 100;     // R
                base[idx as usize + 1] = 150; // G
                base[idx as usize + 2] = 200; // B
                base[idx as usize + 3] = 255; // A
            }
        }

        let mips = generate_mips(width, height, &base);

        // Expected levels: 4x2, 2x1, 1x1
        assert_eq!(mips.len(), 3);
        
        // Level 0: 4x2
        assert_eq!(mips[0].len(), (4 * 2 * 4) as usize);
        
        // Level 1: 2x1
        assert_eq!(mips[1].len(), (2 * 4) as usize);
        
        // Level 2: 1x1
        assert_eq!(mips[2].len(), 4);

        // Check averages for level 1 (2x1)
        // Each texel in level 1 is the average of 2x2 texels from level 0
        // Since height is 2, each row in level 1 corresponds to both rows in level 0
        // Width 4 -> width 2, so each texel in level 1 is average of 2 adjacent texels in each row
        // Actually, for 4x2 -> 2x1:
        //   dst[0,0] averages src[0,0], src[1,0], src[0,1], src[1,1]
        //   dst[1,0] averages src[2,0], src[3,0], src[2,1], src[3,1]
        
        // All src texels are 100,150,200,255
        // So avg should be 100,150,200,255
        
        assert_eq!(mips[1][0], 100);
        assert_eq!(mips[1][1], 150);
        assert_eq!(mips[1][2], 200);
        assert_eq!(mips[1][3], 255);
        assert_eq!(mips[1][4], 100);
        assert_eq!(mips[1][5], 150);
        assert_eq!(mips[1][6], 200);
        assert_eq!(mips[1][7], 255);

        // Level 2: 1x1
        // Averages all 4 texels from level 1
        // All are 100,150,200,255, so avg is 100,150,200,255
        assert_eq!(mips[2][0], 100);
        assert_eq!(mips[2][1], 150);
        assert_eq!(mips[2][2], 200);
        assert_eq!(mips[2][3], 255);
    }
}
