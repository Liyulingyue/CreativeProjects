use crate::backend::{BoundingBox, Detection, Error};
use std::marker::PhantomData;

pub struct PipelineResult<L> {
    pub bounding_box: BoundingBox,
    pub landmarks: L,
}

pub trait DetectorResult {
    fn bounding_box(&self) -> &BoundingBox;
}

impl DetectorResult for Detection {
    fn bounding_box(&self) -> &BoundingBox {
        &self.bounding_box
    }
}

pub struct ROICropper;

impl ROICropper {
    pub fn crop(
        image_data: &[u8],
        img_width: u32,
        img_height: u32,
        bbox: &BoundingBox,
    ) -> Result<(Vec<u8>, u32, u32), Error> {
        let x1 = bbox.left.max(0.0) as u32;
        let y1 = bbox.top.max(0.0) as u32;
        let x2 = (bbox.right.min(img_width as f32) as u32).min(img_width);
        let y2 = (bbox.bottom.min(img_height as f32) as u32).min(img_height);

        let roi_width = x2.saturating_sub(x1);
        let roi_height = y2.saturating_sub(y1);

        if roi_width == 0 || roi_height == 0 {
            return Err(Error::InvalidArgument("Invalid ROI dimensions".into()));
        }

        let channels = 3;
        let mut roi_data = Vec::with_capacity((roi_width * roi_height * channels) as usize);

        for y in y1..y2 {
            for x in x1..x2 {
                let idx = ((y * img_width + x) * channels) as usize;
                if idx + 2 < image_data.len() {
                    roi_data.push(image_data[idx]);
                    roi_data.push(image_data[idx + 1]);
                    roi_data.push(image_data[idx + 2]);
                }
            }
        }

        Ok((roi_data, roi_width, roi_height))
    }

    pub fn crop_multiple(
        image_data: &[u8],
        img_width: u32,
        img_height: u32,
        bboxes: &[BoundingBox],
    ) -> Result<Vec<(Vec<u8>, u32, u32)>, Error> {
        bboxes
            .iter()
            .map(|bbox| Self::crop(image_data, img_width, img_height, bbox))
            .collect()
    }
}

pub struct Pipeline<Backend> {
    _phantom: PhantomData<Backend>,
}

impl<Backend> Pipeline<Backend> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<Backend> Default for Pipeline<Backend> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FaceLandmarkWithBox {
    pub bounding_box: BoundingBox,
    pub face_landmarks: Vec<crate::backend::Landmark>,
    pub blendshapes: Option<Vec<crate::backend::Class>>,
}

pub struct HandLandmarkWithBox {
    pub bounding_box: BoundingBox,
    pub hand_landmarks: Vec<crate::backend::Landmark>,
    pub handedness: crate::backend::Class,
}

pub struct PoseLandmarkWithBox {
    pub bounding_box: BoundingBox,
    pub pose_landmarks: Vec<crate::backend::Landmark>,
}

pub fn run_face_pipeline(
    image_data: &[u8],
    img_width: u32,
    img_height: u32,
    detections: &[Detection],
    extract_landmarks: impl Fn(&[u8], u32, u32) -> Result<Vec<crate::backend::Landmark>, Error>,
) -> Result<Vec<FaceLandmarkWithBox>, Error> {
    let bboxes: Vec<BoundingBox> = detections.iter().map(|d| d.bounding_box.clone()).collect();
    let crops = ROICropper::crop_multiple(image_data, img_width, img_height, &bboxes)?;

    let mut results = Vec::new();
    for (i, crop) in crops.into_iter().enumerate() {
        let landmarks = extract_landmarks(&crop.0, crop.1, crop.2)?;
        results.push(FaceLandmarkWithBox {
            bounding_box: detections[i].bounding_box.clone(),
            face_landmarks: landmarks,
            blendshapes: None,
        });
    }

    Ok(results)
}

pub fn run_hand_pipeline(
    image_data: &[u8],
    img_width: u32,
    img_height: u32,
    detections: &[Detection],
    extract_landmarks: impl Fn(&[u8], u32, u32) -> Result<(Vec<crate::backend::Landmark>, crate::backend::Class), Error>,
) -> Result<Vec<HandLandmarkWithBox>, Error> {
    let bboxes: Vec<BoundingBox> = detections.iter().map(|d| d.bounding_box.clone()).collect();
    let crops = ROICropper::crop_multiple(image_data, img_width, img_height, &bboxes)?;

    let mut results = Vec::new();
    for (i, crop) in crops.into_iter().enumerate() {
        let (landmarks, handedness) = extract_landmarks(&crop.0, crop.1, crop.2)?;
        results.push(HandLandmarkWithBox {
            bounding_box: detections[i].bounding_box.clone(),
            hand_landmarks: landmarks,
            handedness,
        });
    }

    Ok(results)
}
