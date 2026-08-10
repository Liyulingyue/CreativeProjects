use image::{Rgb, RgbImage};
use crate::node::{Keypoint, Person};

pub struct Visualizer {
    skeleton: Vec<(usize, usize)>,
    joint_colors: Vec<Rgb<u8>>,
    line_color: Rgb<u8>,
}

impl Visualizer {
    pub fn coco17() -> Self {
        let skeleton = vec![
            (0, 1), (0, 2), (1, 3), (2, 4),
            (5, 6), (5, 7), (7, 9), (6, 8), (8, 10),
            (5, 11), (6, 12), (11, 12),
            (11, 13), (13, 15), (12, 14), (14, 16),
        ];
        let joint_colors = vec![
            Rgb([255, 0, 0]),    // 0: nose - red
            Rgb([255, 255, 0]),  // 1: right_eye - yellow
            Rgb([255, 255, 0]),  // 2: left_eye - yellow
            Rgb([255, 128, 0]),  // 3: right_ear - orange
            Rgb([255, 128, 0]),  // 4: left_ear - orange
            Rgb([0, 255, 0]),    // 5: right_shoulder - green
            Rgb([0, 255, 0]),    // 6: left_shoulder - green
            Rgb([0, 255, 128]),  // 7: right_elbow - teal
            Rgb([0, 255, 128]),  // 8: left_elbow - teal
            Rgb([0, 128, 255]),  // 9: right_wrist - sky blue
            Rgb([0, 128, 255]),  // 10: left_wrist - sky blue
            Rgb([128, 0, 255]),  // 11: right_hip - purple
            Rgb([128, 0, 255]),  // 12: left_hip - purple
            Rgb([255, 0, 255]),  // 13: right_knee - magenta
            Rgb([255, 0, 255]),  // 14: left_knee - magenta
            Rgb([255, 0, 128]),  // 15: right_ankle - pink
            Rgb([255, 0, 128]),  // 16: left_ankle - pink
        ];
        let line_color = Rgb([255, 255, 255]);
        Self { skeleton, joint_colors, line_color }
    }

    pub fn wholebody133() -> Self {
        let skeleton = vec![
            (0, 1), (1, 2), (2, 3), (3, 4),
            (4, 5), (5, 6), (6, 7), (7, 8),
            (8, 9), (9, 10), (10, 11), (11, 12),
            (12, 13), (13, 14), (14, 15), (15, 16),
            (16, 17), (17, 18), (18, 19), (19, 20),
            (20, 21), (21, 22), (22, 23),
            (23, 24), (24, 25), (25, 26), (26, 27),
            (27, 28), (28, 29), (29, 31), (30, 31),
            (31, 32), (32, 33), (33, 34), (34, 35),
            (35, 36), (36, 37), (37, 38), (38, 39),
            (39, 40), (40, 41), (41, 42), (42, 43),
            (43, 44), (44, 45), (45, 47), (46, 47),
            (47, 48), (48, 49), (49, 50), (50, 51),
            (51, 52), (52, 53), (53, 54), (54, 55),
            (55, 56), (56, 57), (57, 58), (58, 59),
            (59, 60), (60, 61), (61, 62), (62, 63),
            (63, 64), (64, 65), (65, 67), (66, 67),
            (67, 68), (68, 69), (69, 70), (70, 71),
            (71, 72), (72, 73), (73, 74), (74, 75),
            (75, 76), (76, 77), (77, 78), (78, 79),
            (79, 80), (80, 81), (81, 82), (82, 83),
            (84, 85), (85, 86), (86, 87), (87, 88),
            (88, 89), (89, 90), (90, 91), (91, 92),
            (92, 93), (93, 94), (94, 95), (95, 96),
            (96, 97), (97, 98), (98, 99), (99, 100),
            (100, 101), (101, 102), (102, 103), (103, 104),
            (104, 105), (105, 106), (106, 107), (107, 108),
            (108, 109), (109, 110), (110, 111), (111, 112),
            (112, 113), (113, 114), (114, 115), (115, 116),
            (116, 117), (117, 118), (118, 119), (119, 120),
            (120, 121), (121, 122), (122, 123), (123, 124),
            (124, 125), (125, 126), (126, 127), (127, 128),
            (128, 129), (129, 130), (130, 131), (131, 132),
        ];
        let joint_colors = vec![Rgb([0, 255, 0]); 133];
        let line_color = Rgb([255, 255, 255]);
        Self { skeleton, joint_colors, line_color }
    }

    pub fn face68() -> Self {
        let skeleton = vec![
            (0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6), (6, 7), (7, 8), (8, 9), (9, 10), (10, 11), (11, 12), (12, 13), (13, 14), (14, 15), (15, 16),
            (17, 18), (18, 19), (19, 20), (20, 21),
            (22, 23), (23, 24), (24, 25), (25, 26), (26, 27), (27, 28), (28, 29), (29, 30),
            (31, 32), (33, 34), (34, 35), (35, 36), (36, 37), (37, 38), (38, 39), (39, 40), (40, 41), (41, 36), (36, 31),
            (42, 43), (43, 44), (44, 45), (45, 46), (46, 47), (47, 48), (48, 49), (49, 50), (50, 51), (51, 52), (52, 53), (53, 54), (54, 55), (55, 56), (56, 57), (57, 58), (58, 59), (59, 48), (48, 42),
            (60, 61), (61, 62), (62, 63), (63, 64), (64, 65), (65, 66), (66, 67), (67, 60),
        ];
        let joint_colors = vec![Rgb([255, 200, 0]); 68];
        let line_color = Rgb([255, 255, 0]);
        Self { skeleton, joint_colors, line_color }
    }

    pub fn hand21() -> Self {
        let skeleton = vec![
            (0, 1), (1, 2), (2, 3), (3, 4),
            (0, 5), (5, 6), (6, 7), (7, 8),
            (0, 9), (9, 10), (10, 11), (11, 12),
            (0, 13), (13, 14), (14, 15), (15, 16),
            (0, 17), (17, 18), (18, 19), (19, 20),
        ];
        let joint_colors = vec![Rgb([100, 200, 255]); 21];
        let line_color = Rgb([200, 200, 200]);
        Self { skeleton, joint_colors, line_color }
    }

    pub fn draw_bbox(&self, image: &mut RgbImage, bbox: &[f32; 4]) {
        let [x1, y1, x2, y2] = *bbox;
        let x1 = x1 as u32;
        let y1 = y1 as u32;
        let x2 = x2 as u32;
        let y2 = y2 as u32;
        let color = Rgb([0, 255, 0]);

        for x in x1..=x2 {
            if x < image.width() && y1 < image.height() {
                image.put_pixel(x, y1, color);
            }
            if x < image.width() && y2 < image.height() {
                image.put_pixel(x, y2, color);
            }
        }
        for y in y1..=y2 {
            if x1 < image.width() && y < image.height() {
                image.put_pixel(x1, y, color);
            }
            if x2 < image.width() && y < image.height() {
                image.put_pixel(x2, y, color);
            }
        }
    }

    pub fn draw_keypoints(&self, image: &mut RgbImage, keypoints: &[Keypoint], radius: u32) {
        for (i, kp) in keypoints.iter().enumerate() {
            if kp.confidence < 0.3 {
                continue;
            }
            let color = if i < self.joint_colors.len() {
                self.joint_colors[i]
            } else {
                Rgb([255, 255, 0])
            };
            let x = kp.x as i32;
            let y = kp.y as i32;
            for dy in -(radius as i32)..=(radius as i32) {
                for dx in -(radius as i32)..=(radius as i32) {
                    if dx * dx + dy * dy <= (radius as i32) * (radius as i32) {
                        let px = x + dx;
                        let py = y + dy;
                        if px >= 0 && (px as u32) < image.width() && py >= 0 && (py as u32) < image.height() {
                            image.put_pixel(px as u32, py as u32, color);
                        }
                    }
                }
            }
        }
    }

    pub fn draw_skeleton(&self, image: &mut RgbImage, keypoints: &[Keypoint]) {
        for &(i, j) in &self.skeleton {
            if i >= keypoints.len() || j >= keypoints.len() {
                continue;
            }
            let kp1 = &keypoints[i];
            let kp2 = &keypoints[j];
            if kp1.confidence < 0.3 || kp2.confidence < 0.3 {
                continue;
            }
            self.draw_line(image, kp1.x as f32, kp1.y as f32, kp2.x as f32, kp2.y as f32, self.line_color);
        }
    }

    pub fn draw_person(&self, image: &mut RgbImage, person: &Person) {
        self.draw_bbox(image, &person.bbox);
        self.draw_keypoints(image, &person.keypoints, 3);
        self.draw_skeleton(image, &person.keypoints);
    }

    pub fn draw_keypoints_colored(
        &self,
        image: &mut RgbImage,
        keypoints: &[Keypoint],
        radius: u32,
        color: (u8, u8, u8),
    ) {
        let color = Rgb([color.0, color.1, color.2]);
        for kp in keypoints.iter() {
            if kp.confidence < 0.3 {
                continue;
            }
            let x = kp.x as i32;
            let y = kp.y as i32;
            for dy in -(radius as i32)..=(radius as i32) {
                for dx in -(radius as i32)..=(radius as i32) {
                    if dx * dx + dy * dy <= (radius as i32) * (radius as i32) {
                        let px = x + dx;
                        let py = y + dy;
                        if px >= 0
                            && (px as u32) < image.width()
                            && py >= 0
                            && (py as u32) < image.height()
                        {
                            image.put_pixel(px as u32, py as u32, color);
                        }
                    }
                }
            }
        }
    }

    pub fn draw_skeleton_colored(
        &self,
        image: &mut RgbImage,
        keypoints: &[Keypoint],
        color: (u8, u8, u8),
    ) {
        let color = Rgb([color.0, color.1, color.2]);
        for &(i, j) in &self.skeleton {
            if i >= keypoints.len() || j >= keypoints.len() {
                continue;
            }
            let kp1 = &keypoints[i];
            let kp2 = &keypoints[j];
            if kp1.confidence < 0.3 || kp2.confidence < 0.3 {
                continue;
            }
            self.draw_line(
                image,
                kp1.x as f32,
                kp1.y as f32,
                kp2.x as f32,
                kp2.y as f32,
                color,
            );
        }
    }

    fn draw_line(&self, image: &mut RgbImage, x1: f32, y1: f32, x2: f32, y2: f32, color: Rgb<u8>) {
        let x1 = x1.round() as i64;
        let y1 = y1.round() as i64;
        let x2 = x2.round() as i64;
        let y2 = y2.round() as i64;
        let dx = (x2 - x1).abs();
        let dy = (y2 - y1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx - dy;
        let mut x = x1;
        let mut y = y1;
        let w = image.width() as i64;
        let h = image.height() as i64;
        loop {
            if x >= 0 && x < w && y >= 0 && y < h {
                image.put_pixel(x as u32, y as u32, color);
            }
            if x == x2 && y == y2 {
                break;
            }
            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
    }
}

impl Default for Visualizer {
    fn default() -> Self {
        Self::coco17()
    }
}
