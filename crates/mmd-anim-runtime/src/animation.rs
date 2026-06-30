use glam::{Quat, Vec3A};

use crate::{BoneIndex, MorphIndex, PoseArena};

const BEZIER_ITERATIONS: usize = 12;
const MMD_INTERPOLATION_SCALE: f32 = 1.0 / 127.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterpolationScalar {
    pub x1: u8,
    pub y1: u8,
    pub x2: u8,
    pub y2: u8,
}

impl InterpolationScalar {
    pub const fn linear() -> Self {
        Self {
            x1: 20,
            y1: 20,
            x2: 107,
            y2: 107,
        }
    }

    pub fn evaluate(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        if self.x1 == self.y1 && self.x2 == self.y2 {
            return x;
        }
        bezier_interpolation(
            self.x1 as f32 * MMD_INTERPOLATION_SCALE,
            self.x2 as f32 * MMD_INTERPOLATION_SCALE,
            self.y1 as f32 * MMD_INTERPOLATION_SCALE,
            self.y2 as f32 * MMD_INTERPOLATION_SCALE,
            x,
        )
    }
}

impl Default for InterpolationScalar {
    fn default() -> Self {
        Self::linear()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterpolationVector3 {
    pub x: InterpolationScalar,
    pub y: InterpolationScalar,
    pub z: InterpolationScalar,
}

impl InterpolationVector3 {
    pub const fn linear() -> Self {
        Self {
            x: InterpolationScalar::linear(),
            y: InterpolationScalar::linear(),
            z: InterpolationScalar::linear(),
        }
    }
}

impl Default for InterpolationVector3 {
    fn default() -> Self {
        Self::linear()
    }
}

#[derive(Clone, Debug)]
pub struct MovableBoneKeyframe {
    pub frame: u32,
    pub position: Vec3A,
    pub rotation: Quat,
    pub position_interpolation: InterpolationVector3,
    pub rotation_interpolation: InterpolationScalar,
}

impl MovableBoneKeyframe {
    pub fn new(frame: u32, position: Vec3A, rotation: Quat) -> Self {
        Self {
            frame,
            position,
            rotation,
            position_interpolation: InterpolationVector3::linear(),
            rotation_interpolation: InterpolationScalar::linear(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MovableBoneTrack {
    frame_numbers: Box<[u32]>,
    positions: Box<[Vec3A]>,
    rotations: Box<[Quat]>,
    position_interpolations: Box<[InterpolationVector3]>,
    rotation_interpolations: Box<[InterpolationScalar]>,
}

impl MovableBoneTrack {
    pub fn from_keyframes(mut keyframes: Vec<MovableBoneKeyframe>) -> Self {
        keyframes.sort_by_key(|keyframe| keyframe.frame);

        let mut frame_numbers = Vec::with_capacity(keyframes.len());
        let mut positions = Vec::with_capacity(keyframes.len());
        let mut rotations = Vec::with_capacity(keyframes.len());
        let mut position_interpolations = Vec::with_capacity(keyframes.len());
        let mut rotation_interpolations = Vec::with_capacity(keyframes.len());

        for keyframe in keyframes {
            frame_numbers.push(keyframe.frame);
            positions.push(keyframe.position);
            rotations.push(keyframe.rotation.normalize());
            position_interpolations.push(keyframe.position_interpolation);
            rotation_interpolations.push(keyframe.rotation_interpolation);
        }

        Self {
            frame_numbers: frame_numbers.into_boxed_slice(),
            positions: positions.into_boxed_slice(),
            rotations: rotations.into_boxed_slice(),
            position_interpolations: position_interpolations.into_boxed_slice(),
            rotation_interpolations: rotation_interpolations.into_boxed_slice(),
        }
    }

    pub fn sample(&self, frame: f32) -> Option<(Vec3A, Quat)> {
        match self.frame_numbers.len() {
            0 => None,
            1 => Some((self.positions[0], self.rotations[0])),
            _ => {
                let next_index = self.find_next_keyframe(frame);
                if next_index == 0 {
                    return Some((self.positions[0], self.rotations[0]));
                }
                if next_index >= self.frame_numbers.len() {
                    let last = self.frame_numbers.len() - 1;
                    return Some((self.positions[last], self.rotations[last]));
                }

                let prev_index = next_index - 1;
                let prev_frame = self.frame_numbers[prev_index] as f32;
                let next_frame = self.frame_numbers[next_index] as f32;
                let frame_t = if next_frame == prev_frame {
                    0.0
                } else {
                    ((frame - prev_frame) / (next_frame - prev_frame)).clamp(0.0, 1.0)
                };

                let interpolation = self.position_interpolations[next_index];
                let position = Vec3A::new(
                    lerp(
                        self.positions[prev_index].x,
                        self.positions[next_index].x,
                        interpolation.x.evaluate(frame_t),
                    ),
                    lerp(
                        self.positions[prev_index].y,
                        self.positions[next_index].y,
                        interpolation.y.evaluate(frame_t),
                    ),
                    lerp(
                        self.positions[prev_index].z,
                        self.positions[next_index].z,
                        interpolation.z.evaluate(frame_t),
                    ),
                );

                let rotation_t = self.rotation_interpolations[next_index].evaluate(frame_t);
                let rotation =
                    self.rotations[prev_index].slerp(self.rotations[next_index], rotation_t);

                Some((position, rotation))
            }
        }
    }

    fn find_next_keyframe(&self, frame: f32) -> usize {
        self.frame_numbers
            .partition_point(|keyframe| (*keyframe as f32) <= frame)
    }

    fn frame_range(&self) -> Option<(u32, u32)> {
        Some((*self.frame_numbers.first()?, *self.frame_numbers.last()?))
    }
}

#[derive(Clone, Debug)]
pub struct BoneAnimationBinding {
    pub bone: BoneIndex,
    pub track: MovableBoneTrack,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MorphKeyframe {
    pub frame: u32,
    pub weight: f32,
}

impl MorphKeyframe {
    pub fn new(frame: u32, weight: f32) -> Self {
        Self { frame, weight }
    }
}

#[derive(Clone, Debug)]
pub struct MorphTrack {
    frame_numbers: Box<[u32]>,
    weights: Box<[f32]>,
}

impl MorphTrack {
    pub fn from_keyframes(mut keyframes: Vec<MorphKeyframe>) -> Self {
        keyframes.sort_by_key(|keyframe| keyframe.frame);
        let mut frame_numbers = Vec::with_capacity(keyframes.len());
        let mut weights = Vec::with_capacity(keyframes.len());
        for keyframe in keyframes {
            frame_numbers.push(keyframe.frame);
            weights.push(keyframe.weight);
        }
        Self {
            frame_numbers: frame_numbers.into_boxed_slice(),
            weights: weights.into_boxed_slice(),
        }
    }

    pub fn sample(&self, frame: f32) -> Option<f32> {
        match self.frame_numbers.len() {
            0 => None,
            1 => Some(self.weights[0]),
            _ => {
                let next_index = self
                    .frame_numbers
                    .partition_point(|keyframe| (*keyframe as f32) <= frame);
                if next_index == 0 {
                    return Some(self.weights[0]);
                }
                if next_index >= self.frame_numbers.len() {
                    return Some(self.weights[self.weights.len() - 1]);
                }

                let prev_index = next_index - 1;
                let prev_frame = self.frame_numbers[prev_index] as f32;
                let next_frame = self.frame_numbers[next_index] as f32;
                let frame_t = if next_frame == prev_frame {
                    0.0
                } else {
                    ((frame - prev_frame) / (next_frame - prev_frame)).clamp(0.0, 1.0)
                };
                Some(lerp(
                    self.weights[prev_index],
                    self.weights[next_index],
                    frame_t,
                ))
            }
        }
    }

    fn frame_range(&self) -> Option<(u32, u32)> {
        Some((*self.frame_numbers.first()?, *self.frame_numbers.last()?))
    }
}

#[derive(Clone, Debug)]
pub struct MorphAnimationBinding {
    pub morph: MorphIndex,
    pub track: MorphTrack,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyKeyframe {
    pub frame: u32,
    pub ik_enabled: Box<[u8]>,
}

impl PropertyKeyframe {
    pub fn new(frame: u32, ik_enabled: Vec<bool>) -> Self {
        Self {
            frame,
            ik_enabled: ik_enabled
                .into_iter()
                .map(u8::from)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PropertyAnimationBinding {
    frame_numbers: Box<[u32]>,
    ik_enabled: Box<[Box<[u8]>]>,
}

impl PropertyAnimationBinding {
    pub fn from_keyframes(mut keyframes: Vec<PropertyKeyframe>) -> Self {
        keyframes.sort_by_key(|keyframe| keyframe.frame);

        let mut frame_numbers = Vec::with_capacity(keyframes.len());
        let mut ik_enabled = Vec::with_capacity(keyframes.len());
        for keyframe in keyframes {
            frame_numbers.push(keyframe.frame);
            ik_enabled.push(keyframe.ik_enabled);
        }

        Self {
            frame_numbers: frame_numbers.into_boxed_slice(),
            ik_enabled: ik_enabled.into_boxed_slice(),
        }
    }

    pub fn sample(&self, frame: f32) -> Option<&[u8]> {
        match self.frame_numbers.len() {
            0 => None,
            _ => {
                let next_index = self
                    .frame_numbers
                    .partition_point(|keyframe| (*keyframe as f32) <= frame);
                if next_index == 0 {
                    None
                } else {
                    Some(&self.ik_enabled[next_index - 1])
                }
            }
        }
    }

    fn frame_range(&self) -> Option<(u32, u32)> {
        Some((*self.frame_numbers.first()?, *self.frame_numbers.last()?))
    }
}

#[derive(Clone, Debug, Default)]
pub struct AnimationClip {
    bone_tracks: Box<[BoneAnimationBinding]>,
    morph_tracks: Box<[MorphAnimationBinding]>,
    property_track: Option<PropertyAnimationBinding>,
}

impl AnimationClip {
    pub fn new(bone_tracks: Vec<BoneAnimationBinding>) -> Self {
        Self::new_with_morphs(bone_tracks, Vec::new())
    }

    pub fn new_with_morphs(
        bone_tracks: Vec<BoneAnimationBinding>,
        morph_tracks: Vec<MorphAnimationBinding>,
    ) -> Self {
        Self::new_full(bone_tracks, morph_tracks, None)
    }

    pub fn new_full(
        bone_tracks: Vec<BoneAnimationBinding>,
        morph_tracks: Vec<MorphAnimationBinding>,
        property_track: Option<PropertyAnimationBinding>,
    ) -> Self {
        Self {
            bone_tracks: bone_tracks.into_boxed_slice(),
            morph_tracks: morph_tracks.into_boxed_slice(),
            property_track,
        }
    }

    pub fn apply_to_pose(&self, frame: f32, pose: &mut PoseArena) {
        pose.reset_local_pose();
        for binding in self.bone_tracks.iter() {
            if let Some((position, rotation)) = binding.track.sample(frame) {
                pose.set_local_position_offset(binding.bone, position);
                pose.set_local_rotation(binding.bone, rotation);
            }
        }
        for binding in self.morph_tracks.iter() {
            if let Some(weight) = binding.track.sample(frame) {
                pose.set_morph_weight(binding.morph, weight);
            }
        }
        if let Some(ik_enabled) = self
            .property_track
            .as_ref()
            .and_then(|track| track.sample(frame))
        {
            for (ik_index, enabled) in ik_enabled.iter().enumerate() {
                pose.set_ik_enabled(ik_index, *enabled != 0);
            }
        }
    }

    pub fn bone_track_count(&self) -> usize {
        self.bone_tracks.len()
    }

    pub fn morph_track_count(&self) -> usize {
        self.morph_tracks.len()
    }

    pub fn has_property_track(&self) -> bool {
        self.property_track.is_some()
    }

    pub fn frame_range(&self) -> Option<(u32, u32)> {
        let mut range: Option<(u32, u32)> = None;
        for binding in self.bone_tracks.iter() {
            merge_frame_range(&mut range, binding.track.frame_range());
        }
        for binding in self.morph_tracks.iter() {
            merge_frame_range(&mut range, binding.track.frame_range());
        }
        if let Some(property_track) = self.property_track.as_ref() {
            merge_frame_range(&mut range, property_track.frame_range());
        }
        range
    }

    pub fn find_bone_track(&self, bone: BoneIndex) -> Option<&MovableBoneTrack> {
        self.bone_tracks
            .iter()
            .find(|binding| binding.bone == bone)
            .map(|binding| &binding.track)
    }
}

fn merge_frame_range(target: &mut Option<(u32, u32)>, range: Option<(u32, u32)>) {
    let Some((first, last)) = range else {
        return;
    };
    *target = Some(match *target {
        Some((current_first, current_last)) => (current_first.min(first), current_last.max(last)),
        None => (first, last),
    });
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn bezier_interpolation(x1: f32, x2: f32, y1: f32, y2: f32, x: f32) -> f32 {
    let mut c = 0.5;
    let mut t = c;
    let mut s = 1.0 - t;

    let mut sst3;
    let mut stt3;
    let mut ttt;

    for _ in 0..BEZIER_ITERATIONS {
        sst3 = 3.0 * s * s * t;
        stt3 = 3.0 * s * t * t;
        ttt = t * t * t;

        let ft = sst3 * x1 + stt3 * x2 + ttt - x;
        if ft == 0.0 {
            return sst3 * y1 + stt3 * y2 + ttt;
        }

        c *= 0.5;
        t += if ft < 0.0 { c } else { -c };
        s = 1.0 - t;
    }

    sst3 = 3.0 * s * s * t;
    stt3 = 3.0 * s * t * t;
    ttt = t * t * t;
    sst3 * y1 + stt3 * y2 + ttt
}

#[cfg(test)]
mod tests {
    use glam::{Quat, Vec3A};

    use super::*;

    fn assert_near(actual: f32, expected: f32) {
        let delta = (actual - expected).abs();
        assert!(
            delta < 1.0e-4,
            "actual={actual:?} expected={expected:?} delta={delta:?}"
        );
    }

    fn assert_vec3a_near(actual: Vec3A, expected: Vec3A) {
        let delta = (actual - expected).abs();
        assert!(
            delta.x < 1.0e-4 && delta.y < 1.0e-4 && delta.z < 1.0e-4,
            "actual={actual:?} expected={expected:?} delta={delta:?}"
        );
    }

    #[test]
    fn linear_interpolation_maps_half_to_half() {
        assert_near(InterpolationScalar::linear().evaluate(0.5), 0.5);
    }

    #[test]
    fn mmd_camera_ease_out_matches_native_subdivision_points() {
        let interpolation = InterpolationScalar {
            x1: 0,
            y1: 127,
            x2: 127,
            y2: 127,
        };

        assert_near(interpolation.evaluate(1.0 / 6.0), 0.5933867);
        assert_near(interpolation.evaluate(1.0 / 3.0), 0.76974934);
        assert_near(interpolation.evaluate(0.5), 0.875);
        assert_near(interpolation.evaluate(2.0 / 3.0), 0.9420012);
        assert_near(interpolation.evaluate(5.0 / 6.0), 0.9825947);
    }

    #[test]
    fn samples_movable_bone_track() {
        let track = MovableBoneTrack::from_keyframes(vec![
            MovableBoneKeyframe::new(20, Vec3A::new(10.0, 0.0, 0.0), Quat::IDENTITY),
            MovableBoneKeyframe::new(10, Vec3A::ZERO, Quat::IDENTITY),
        ]);

        let (position, rotation) = track.sample(15.0).unwrap();

        assert_vec3a_near(position, Vec3A::new(5.0, 0.0, 0.0));
        assert_near(rotation.dot(Quat::IDENTITY).abs(), 1.0);
    }

    #[test]
    fn samples_morph_track() {
        let track = MorphTrack::from_keyframes(vec![
            MorphKeyframe::new(60, 1.0),
            MorphKeyframe::new(0, 0.0),
        ]);

        assert_near(track.sample(30.0).unwrap(), 0.5);
    }

    #[test]
    fn samples_property_track_as_step_state() {
        let track = PropertyAnimationBinding::from_keyframes(vec![
            PropertyKeyframe::new(30, vec![false, true]),
            PropertyKeyframe::new(0, vec![true, true]),
        ]);

        assert_eq!(track.sample(-1.0), None);
        assert_eq!(track.sample(29.0).unwrap(), &[1, 1]);
        assert_eq!(track.sample(30.0).unwrap(), &[0, 1]);
        assert_eq!(track.sample(60.0).unwrap(), &[0, 1]);
    }

    #[test]
    fn property_track_returns_none_before_first_keyframe() {
        let track =
            PropertyAnimationBinding::from_keyframes(vec![PropertyKeyframe::new(30, vec![false])]);

        assert_eq!(track.sample(29.0), None);
        assert_eq!(track.sample(30.0).unwrap(), &[0]);
    }

    #[test]
    fn clip_frame_range_spans_all_track_types() {
        let bone_track = BoneAnimationBinding {
            bone: BoneIndex(0),
            track: MovableBoneTrack::from_keyframes(vec![
                MovableBoneKeyframe::new(30, Vec3A::ZERO, Quat::IDENTITY),
                MovableBoneKeyframe::new(10, Vec3A::ZERO, Quat::IDENTITY),
            ]),
        };
        let morph_track = MorphAnimationBinding {
            morph: MorphIndex(0),
            track: MorphTrack::from_keyframes(vec![
                MorphKeyframe::new(20, 0.0),
                MorphKeyframe::new(60, 1.0),
            ]),
        };
        let property_track = PropertyAnimationBinding::from_keyframes(vec![
            PropertyKeyframe::new(5, vec![true]),
            PropertyKeyframe::new(40, vec![false]),
        ]);
        let clip =
            AnimationClip::new_full(vec![bone_track], vec![morph_track], Some(property_track));

        assert_eq!(clip.frame_range(), Some((5, 60)));
    }

    #[test]
    fn empty_clip_frame_range_is_none() {
        assert_eq!(AnimationClip::default().frame_range(), None);
    }
}
