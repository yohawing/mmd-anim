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

    pub fn keyframe_count(&self) -> usize {
        self.frame_numbers.len()
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

    pub fn frame_range(&self) -> Option<(u32, u32)> {
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

    pub fn keyframe_count(&self) -> usize {
        self.frame_numbers.len()
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

    pub fn frame_range(&self) -> Option<(u32, u32)> {
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

    pub fn keyframe_count(&self) -> usize {
        self.frame_numbers.len()
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

    pub fn frame_range(&self) -> Option<(u32, u32)> {
        Some((*self.frame_numbers.first()?, *self.frame_numbers.last()?))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneSample {
    pub bone: BoneIndex,
    pub position: Vec3A,
    pub rotation: Quat,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MorphSample {
    pub morph: MorphIndex,
    pub weight: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipSample {
    bone_samples: Vec<BoneSample>,
    morph_samples: Vec<MorphSample>,
    ik_enabled: Option<Vec<u8>>,
}

impl ClipSample {
    pub fn with_capacity(bone_capacity: usize, morph_capacity: usize) -> Self {
        Self {
            bone_samples: Vec::with_capacity(bone_capacity),
            morph_samples: Vec::with_capacity(morph_capacity),
            ik_enabled: None,
        }
    }

    pub fn bone_samples(&self) -> &[BoneSample] {
        &self.bone_samples
    }

    pub fn morph_samples(&self) -> &[MorphSample] {
        &self.morph_samples
    }

    pub fn ik_enabled(&self) -> Option<&[u8]> {
        self.ik_enabled.as_deref()
    }

    pub fn apply_to_pose(&self, pose: &mut PoseArena) {
        pose.reset_local_pose();
        for sample in self.bone_samples.iter() {
            pose.set_local_position_offset(sample.bone, sample.position);
            pose.set_local_rotation(sample.bone, sample.rotation);
        }
        for sample in self.morph_samples.iter() {
            pose.set_morph_weight(sample.morph, sample.weight);
        }
        if let Some(ik_enabled) = self.ik_enabled.as_ref() {
            for (ik_index, enabled) in ik_enabled.iter().enumerate() {
                pose.set_ik_enabled(ik_index, *enabled != 0);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipFrameBounds {
    pub start: f32,
    pub end: f32,
}

impl ClipFrameBounds {
    pub const fn new(start: f32, end: f32) -> Self {
        Self { start, end }
    }
}

enum ClipSampleEvent<'a> {
    Bone(BoneIndex, Vec3A, Quat),
    Morph(MorphIndex, f32),
    IkEnabled(&'a [u8]),
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

    pub fn builder() -> AnimationClipBuilder {
        AnimationClipBuilder::new()
    }

    pub fn sample_at(&self, frame: f32) -> ClipSample {
        let mut sample = ClipSample::with_capacity(self.bone_tracks.len(), self.morph_tracks.len());
        self.sample_into(frame, &mut sample);
        sample
    }

    pub fn sample_into(&self, frame: f32, sample: &mut ClipSample) {
        sample.bone_samples.clear();
        sample.morph_samples.clear();
        let mut ik_enabled = sample.ik_enabled.take();
        let mut has_ik_state = false;
        self.visit_samples(frame, |event| match event {
            ClipSampleEvent::Bone(bone, position, rotation) => {
                sample.bone_samples.push(BoneSample {
                    bone,
                    position,
                    rotation,
                });
            }
            ClipSampleEvent::Morph(morph, weight) => {
                sample.morph_samples.push(MorphSample { morph, weight });
            }
            ClipSampleEvent::IkEnabled(state) => {
                let buffer = ik_enabled.get_or_insert_with(Vec::new);
                buffer.clear();
                buffer.extend_from_slice(state);
                has_ik_state = true;
            }
        });
        sample.ik_enabled = if has_ik_state { ik_enabled } else { None };
    }

    pub fn apply_to_pose(&self, frame: f32, pose: &mut PoseArena) {
        pose.reset_local_pose();
        self.visit_samples(frame, |event| match event {
            ClipSampleEvent::Bone(bone, position, rotation) => {
                pose.set_local_position_offset(bone, position);
                pose.set_local_rotation(bone, rotation);
            }
            ClipSampleEvent::Morph(morph, weight) => {
                pose.set_morph_weight(morph, weight);
            }
            ClipSampleEvent::IkEnabled(ik_enabled) => {
                for (ik_index, enabled) in ik_enabled.iter().enumerate() {
                    pose.set_ik_enabled(ik_index, *enabled != 0);
                }
            }
        });
    }

    fn visit_samples(&self, frame: f32, mut on_event: impl FnMut(ClipSampleEvent<'_>)) {
        for binding in self.bone_tracks.iter() {
            if let Some((position, rotation)) = binding.track.sample(frame) {
                on_event(ClipSampleEvent::Bone(binding.bone, position, rotation));
            }
        }
        for binding in self.morph_tracks.iter() {
            if let Some(weight) = binding.track.sample(frame) {
                on_event(ClipSampleEvent::Morph(binding.morph, weight));
            }
        }
        if let Some(ik_enabled) = self
            .property_track
            .as_ref()
            .and_then(|track| track.sample(frame))
        {
            on_event(ClipSampleEvent::IkEnabled(ik_enabled));
        }
    }

    pub fn bone_tracks(&self) -> &[BoneAnimationBinding] {
        &self.bone_tracks
    }

    pub fn morph_tracks(&self) -> &[MorphAnimationBinding] {
        &self.morph_tracks
    }

    pub fn property_track(&self) -> Option<&PropertyAnimationBinding> {
        self.property_track.as_ref()
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

    pub fn frame_bounds(&self) -> Option<ClipFrameBounds> {
        self.frame_range()
            .map(|(first, last)| ClipFrameBounds::new(first as f32, last as f32))
    }

    pub fn find_bone_track(&self, bone: BoneIndex) -> Option<&MovableBoneTrack> {
        self.bone_tracks
            .iter()
            .find(|binding| binding.bone == bone)
            .map(|binding| &binding.track)
    }

    pub fn find_morph_track(&self, morph: MorphIndex) -> Option<&MorphTrack> {
        self.morph_tracks
            .iter()
            .find(|binding| binding.morph == morph)
            .map(|binding| &binding.track)
    }
}

#[derive(Clone, Debug, Default)]
pub struct AnimationClipBuilder {
    bone_tracks: Vec<BoneAnimationBinding>,
    morph_tracks: Vec<MorphAnimationBinding>,
    property_track: Option<PropertyAnimationBinding>,
}

impl AnimationClipBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_bone_track(mut self, binding: BoneAnimationBinding) -> Self {
        self.bone_tracks.push(binding);
        self
    }

    pub fn with_morph_track(mut self, binding: MorphAnimationBinding) -> Self {
        self.morph_tracks.push(binding);
        self
    }

    pub fn with_property_track(mut self, track: PropertyAnimationBinding) -> Self {
        self.property_track = Some(track);
        self
    }

    pub fn push_bone_track(&mut self, binding: BoneAnimationBinding) -> &mut Self {
        self.bone_tracks.push(binding);
        self
    }

    pub fn push_morph_track(&mut self, binding: MorphAnimationBinding) -> &mut Self {
        self.morph_tracks.push(binding);
        self
    }

    pub fn set_property_track(&mut self, track: PropertyAnimationBinding) -> &mut Self {
        self.property_track = Some(track);
        self
    }

    pub fn build(self) -> AnimationClip {
        AnimationClip::new_full(self.bone_tracks, self.morph_tracks, self.property_track)
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
    fn clip_frame_bounds_match_integer_frame_range() {
        let clip = AnimationClip::new_full(
            vec![BoneAnimationBinding {
                bone: BoneIndex(0),
                track: MovableBoneTrack::from_keyframes(vec![
                    MovableBoneKeyframe::new(10, Vec3A::ZERO, Quat::IDENTITY),
                    MovableBoneKeyframe::new(20, Vec3A::ZERO, Quat::IDENTITY),
                ]),
            }],
            vec![MorphAnimationBinding {
                morph: MorphIndex(0),
                track: MorphTrack::from_keyframes(vec![
                    MorphKeyframe::new(5, 0.0),
                    MorphKeyframe::new(30, 1.0),
                ]),
            }],
            None,
        );

        assert_eq!(clip.frame_range(), Some((5, 30)));
        assert_eq!(clip.frame_bounds(), Some(ClipFrameBounds::new(5.0, 30.0)));
    }

    #[test]
    fn clip_builder_matches_full_constructor() {
        let bone_track = BoneAnimationBinding {
            bone: BoneIndex(0),
            track: MovableBoneTrack::from_keyframes(vec![
                MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
                MovableBoneKeyframe::new(10, Vec3A::new(10.0, 0.0, 0.0), Quat::IDENTITY),
            ]),
        };
        let morph_track = MorphAnimationBinding {
            morph: MorphIndex(0),
            track: MorphTrack::from_keyframes(vec![
                MorphKeyframe::new(0, 0.0),
                MorphKeyframe::new(10, 1.0),
            ]),
        };
        let property_track = PropertyAnimationBinding::from_keyframes(vec![
            PropertyKeyframe::new(0, vec![true, true]),
            PropertyKeyframe::new(10, vec![false, true]),
        ]);

        let direct = AnimationClip::new_full(
            vec![bone_track.clone()],
            vec![morph_track.clone()],
            Some(property_track.clone()),
        );
        let built = AnimationClip::builder()
            .with_bone_track(bone_track)
            .with_morph_track(morph_track)
            .with_property_track(property_track)
            .build();

        assert_eq!(built.bone_track_count(), direct.bone_track_count());
        assert_eq!(built.morph_track_count(), direct.morph_track_count());
        assert_eq!(built.has_property_track(), direct.has_property_track());
        assert_eq!(built.frame_range(), direct.frame_range());
        assert_eq!(built.sample_at(5.0), direct.sample_at(5.0));
    }

    #[test]
    fn clip_sample_applies_same_pose_as_clip() {
        let clip = AnimationClip::new_full(
            vec![BoneAnimationBinding {
                bone: BoneIndex(0),
                track: MovableBoneTrack::from_keyframes(vec![
                    MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
                    MovableBoneKeyframe::new(10, Vec3A::new(2.0, 4.0, 6.0), Quat::IDENTITY),
                ]),
            }],
            vec![MorphAnimationBinding {
                morph: MorphIndex(0),
                track: MorphTrack::from_keyframes(vec![
                    MorphKeyframe::new(0, 0.0),
                    MorphKeyframe::new(10, 1.0),
                ]),
            }],
            Some(PropertyAnimationBinding::from_keyframes(vec![
                PropertyKeyframe::new(0, vec![true, true]),
                PropertyKeyframe::new(10, vec![false, true]),
            ])),
        );

        let mut from_clip = PoseArena::new_with_counts(1, 1, 2);
        clip.apply_to_pose(5.0, &mut from_clip);

        let mut from_sample = PoseArena::new_with_counts(1, 1, 2);
        let sample = clip.sample_at(5.0);
        sample.apply_to_pose(&mut from_sample);

        assert_vec3a_near(
            from_sample.local_position_offset(BoneIndex(0)),
            from_clip.local_position_offset(BoneIndex(0)),
        );
        assert_near(
            from_sample
                .local_rotation(BoneIndex(0))
                .dot(from_clip.local_rotation(BoneIndex(0))),
            1.0,
        );
        assert_near(
            from_sample.morph_weight(MorphIndex(0)),
            from_clip.morph_weight(MorphIndex(0)),
        );
        assert_eq!(from_sample.ik_enabled(), from_clip.ik_enabled());
    }

    #[test]
    fn clip_sample_into_reuses_output_and_matches_sample_at() {
        let clip = AnimationClip::builder()
            .with_bone_track(BoneAnimationBinding {
                bone: BoneIndex(0),
                track: MovableBoneTrack::from_keyframes(vec![
                    MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
                    MovableBoneKeyframe::new(10, Vec3A::new(10.0, 0.0, 0.0), Quat::IDENTITY),
                ]),
            })
            .with_morph_track(MorphAnimationBinding {
                morph: MorphIndex(0),
                track: MorphTrack::from_keyframes(vec![
                    MorphKeyframe::new(0, 0.0),
                    MorphKeyframe::new(10, 1.0),
                ]),
            })
            .with_property_track(PropertyAnimationBinding::from_keyframes(vec![
                PropertyKeyframe::new(0, vec![true, false]),
                PropertyKeyframe::new(10, vec![false, true]),
            ]))
            .build();

        let expected = clip.sample_at(5.0);
        let mut sample = ClipSample::with_capacity(1, 1);
        clip.sample_into(5.0, &mut sample);
        assert_eq!(sample, expected);

        let bone_capacity = sample.bone_samples.capacity();
        let morph_capacity = sample.morph_samples.capacity();
        let ik_capacity = sample
            .ik_enabled
            .as_ref()
            .expect("property sample should include IK state")
            .capacity();
        clip.sample_into(0.0, &mut sample);
        assert!(sample.bone_samples.capacity() >= bone_capacity);
        assert!(sample.morph_samples.capacity() >= morph_capacity);
        assert!(
            sample
                .ik_enabled
                .as_ref()
                .expect("property sample should include IK state")
                .capacity()
                >= ik_capacity
        );
    }

    #[test]
    fn clip_exposes_track_collections_and_lookup() {
        let clip = AnimationClip::builder()
            .with_bone_track(BoneAnimationBinding {
                bone: BoneIndex(3),
                track: MovableBoneTrack::from_keyframes(vec![MovableBoneKeyframe::new(
                    12,
                    Vec3A::ZERO,
                    Quat::IDENTITY,
                )]),
            })
            .with_morph_track(MorphAnimationBinding {
                morph: MorphIndex(4),
                track: MorphTrack::from_keyframes(vec![MorphKeyframe::new(8, 0.25)]),
            })
            .with_property_track(PropertyAnimationBinding::from_keyframes(vec![
                PropertyKeyframe::new(6, vec![false]),
            ]))
            .build();

        assert_eq!(clip.bone_tracks().len(), 1);
        assert_eq!(clip.bone_tracks()[0].bone, BoneIndex(3));
        assert_eq!(clip.bone_tracks()[0].track.keyframe_count(), 1);
        assert_eq!(clip.bone_tracks()[0].track.frame_range(), Some((12, 12)));
        assert_eq!(
            clip.find_bone_track(BoneIndex(3)).unwrap().keyframe_count(),
            1
        );

        assert_eq!(clip.morph_tracks().len(), 1);
        assert_eq!(clip.morph_tracks()[0].morph, MorphIndex(4));
        assert_eq!(clip.morph_tracks()[0].track.keyframe_count(), 1);
        assert_eq!(clip.morph_tracks()[0].track.frame_range(), Some((8, 8)));
        assert_eq!(
            clip.find_morph_track(MorphIndex(4))
                .unwrap()
                .keyframe_count(),
            1
        );

        let property_track = clip.property_track().unwrap();
        assert_eq!(property_track.keyframe_count(), 1);
        assert_eq!(property_track.frame_range(), Some((6, 6)));
        assert_eq!(property_track.sample(5.0), None);
    }

    #[test]
    fn empty_clip_frame_range_is_none() {
        assert_eq!(AnimationClip::default().frame_range(), None);
        assert_eq!(AnimationClip::default().frame_bounds(), None);
    }
}
