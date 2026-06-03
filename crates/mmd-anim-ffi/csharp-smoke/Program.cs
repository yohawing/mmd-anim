using System.Reflection;
using System.Runtime.InteropServices;

namespace MmdRuntimeFfi.CSharp.Smoke;

internal static class Native
{
    static Native()
    {
        NativeLibrary.SetDllImportResolver(typeof(Native).Assembly, Resolve);
    }

    private static nint Resolve(string name, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (name != "mmd_runtime_ffi")
            return nint.Zero;

        var isWindows = RuntimeInformation.IsOSPlatform(OSPlatform.Windows);
        var isLinux = RuntimeInformation.IsOSPlatform(OSPlatform.Linux);
        var dllName = isWindows ? "mmd_runtime_ffi.dll"
                    : isLinux ? "libmmd_runtime_ffi.so"
                    : "libmmd_runtime_ffi.dylib";

        string? envPath = Environment.GetEnvironmentVariable("MMD_RUNTIME_FFI_PATH");
        if (!string.IsNullOrEmpty(envPath) && File.Exists(envPath))
        {
            if (NativeLibrary.TryLoad(envPath, out nint handle))
                return handle;
        }

        nint h;
        string baseDir = AppContext.BaseDirectory;
        string root = Path.GetFullPath(Path.Combine(baseDir, "../../../../../../"));
        string dllPath = Path.Combine(root, "target", "release", dllName);
        if (File.Exists(dllPath) && NativeLibrary.TryLoad(dllPath, out h))
            return h;

        return nint.Zero;
    }

    // ------------------------------------------------------------------
    //  FFI struct definitions (must match mmd_runtime.h / Rust repr(C))
    // ------------------------------------------------------------------

    [StructLayout(LayoutKind.Sequential)]
    public struct MmdRuntimeFfiBoneTrack
    {
        public uint bone_index;
        public nint keyframe_offset;
        public nint keyframe_count;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MmdRuntimeFfiBoneKeyframe
    {
        public uint frame;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 3)]
        public float[] position_xyz;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)]
        public float[] rotation_xyzw;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MmdRuntimeFfiMorphTrack
    {
        public uint morph_index;
        public nint keyframe_offset;
        public nint keyframe_count;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MmdRuntimeFfiMorphKeyframe
    {
        public uint frame;
        public float weight;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MmdRuntimeFfiPropertyKeyframe
    {
        public uint frame;
        public nint ik_enabled_offset;
        public nint ik_enabled_count;
    }

    [DllImport("mmd_runtime_ffi")]
    public static extern uint mmd_runtime_abi_version();

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_model_create(
        [In] int[] parentIndices,
        [In] float[] restPositionsXyz,
        nint boneCount);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_model_create_from_pmx_bytes(
        [In] byte[] data,
        nint len);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_model_bone_count(nint model);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_model_morph_count(nint model);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_model_ik_count(nint model);

    [DllImport("mmd_runtime_ffi")]
    public static extern void mmd_runtime_model_free(nint model);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_instance_create(nint model, nint morphCount);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_instance_create_for_model(nint model);

    [DllImport("mmd_runtime_ffi")]
    public static extern void mmd_runtime_instance_free(nint instance);

    [DllImport("mmd_runtime_ffi")]
    public static extern byte mmd_runtime_instance_evaluate_rest_pose(nint instance);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_instance_bone_count(nint instance);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_instance_world_matrices(nint instance);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_instance_skinning_matrices(nint instance);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_instance_world_matrix_f32_len(nint instance);

    [DllImport("mmd_runtime_ffi")]
    public static extern byte mmd_runtime_instance_copy_world_matrices(
        nint instance,
        [Out] float[] outF32,
        nint outF32Len);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_instance_skinning_matrix_f32_len(nint instance);

    [DllImport("mmd_runtime_ffi")]
    public static extern byte mmd_runtime_instance_copy_skinning_matrices(
        nint instance,
        [Out] float[] outF32,
        nint outF32Len);

    // --- clip lifecycle ---

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_instance_create_with_counts(
        nint model,
        nint morphCount,
        nint ikCount);

    [DllImport("mmd_runtime_ffi")]
    public static extern byte mmd_runtime_instance_evaluate_clip_frame(
        nint instance,
        nint clip,
        float frame);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_clip_create(
        [In] MmdRuntimeFfiBoneTrack[] boneTracks,
        nint boneTrackCount,
        [In] MmdRuntimeFfiBoneKeyframe[] boneKeyframes,
        nint boneKeyframeCount,
        [In] MmdRuntimeFfiMorphTrack[] morphTracks,
        nint morphTrackCount,
        [In] MmdRuntimeFfiMorphKeyframe[] morphKeyframes,
        nint morphKeyframeCount,
        [In] MmdRuntimeFfiPropertyKeyframe[] propertyKeyframes,
        nint propertyKeyframeCount,
        [In] byte[] propertyIkEnabled,
        nint propertyIkEnabledCount);

    [DllImport("mmd_runtime_ffi")]
    public static extern byte mmd_runtime_clip_frame_range(
        nint clip,
        out uint firstFrame,
        out uint lastFrame);

    [DllImport("mmd_runtime_ffi")]
    public static extern void mmd_runtime_clip_free(nint clip);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_clip_create_from_vmd_bytes_for_model(
        nint model,
        [In] byte[] data,
        nint len);

    // --- morph weight output ---

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_instance_morph_weight_len(nint instance);

    [DllImport("mmd_runtime_ffi")]
    public static extern byte mmd_runtime_instance_copy_morph_weights(
        nint instance,
        [Out] float[] outF32,
        nint outF32Len);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_instance_morph_weights(nint instance);

    // --- IK enabled output ---

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_instance_ik_enabled_len(nint instance);

    [DllImport("mmd_runtime_ffi")]
    public static extern byte mmd_runtime_instance_copy_ik_enabled(
        nint instance,
        [Out] byte[] outU8,
        nint outU8Len);

    [DllImport("mmd_runtime_ffi")]
    public static extern nint mmd_runtime_instance_ik_enabled(nint instance);
}

internal static class Smoke
{
    private static void Assert(bool condition, string message)
    {
        if (!condition)
            throw new InvalidOperationException($"Assertion failed: {message}");
    }

    private static void AssertEqual<T>(T expected, T actual, string label)
        where T : IEquatable<T>
    {
        if (!expected.Equals(actual))
            throw new InvalidOperationException(
                $"Mismatch on {label}: expected={expected}, actual={actual}");
    }

    public static void Run()
    {
        // --- abi_version ---
        uint version = Native.mmd_runtime_abi_version();
        AssertEqual(1u, version, "abi_version");
        Console.WriteLine("  [ok] abi_version == 1");

        byte[] invalidBytes = [0x4d, 0x4d, 0x44];
        nint invalidImportedModel = Native.mmd_runtime_model_create_from_pmx_bytes(
            invalidBytes,
            (nint)invalidBytes.Length);
        AssertEqual(nint.Zero, invalidImportedModel, "invalid PMX byte import returns null");
        Console.WriteLine("  [ok] invalid PMX byte import returns null");

        nint invalidImportedClip = Native.mmd_runtime_clip_create_from_vmd_bytes_for_model(
            nint.Zero,
            invalidBytes,
            (nint)invalidBytes.Length);
        AssertEqual(nint.Zero, invalidImportedClip, "invalid VMD byte import returns null");
        Console.WriteLine("  [ok] invalid VMD byte import returns null");

        // --- create 2-bone model (same as Rust test evaluates_rest_pose_through_c_abi) ---
        int[] parents = [-1, 0];
        float[] restPos = [1.0f, 0.0f, 0.0f, 0.0f, 2.0f, 0.0f];
        nint model = Native.mmd_runtime_model_create(parents, restPos, (nint)2);
        Assert(model != nint.Zero, "model != null");
        Console.WriteLine("  [ok] model created");
        AssertEqual((nint)2, Native.mmd_runtime_model_bone_count(model), "model bone count");
        AssertEqual((nint)0, Native.mmd_runtime_model_morph_count(model), "model morph count");
        AssertEqual((nint)0, Native.mmd_runtime_model_ik_count(model), "model IK count");
        Console.WriteLine("  [ok] model count accessors match flat model");
        nint autoSizedInstance = Native.mmd_runtime_instance_create_for_model(model);
        Assert(autoSizedInstance != nint.Zero, "auto-sized instance created");
        Native.mmd_runtime_instance_free(autoSizedInstance);
        Console.WriteLine("  [ok] auto-sized instance can be created from model counts");

        nint instance = nint.Zero;
        nint clip = nint.Zero;
        try
        {
            instance = Native.mmd_runtime_instance_create_with_counts(model, (nint)1, (nint)1);
            Assert(instance != nint.Zero, "instance != null");
            Console.WriteLine("  [ok] instance created");

            // --- evaluate rest pose ---
            byte ok = Native.mmd_runtime_instance_evaluate_rest_pose(instance);
            Assert(ok != 0, "evaluate_rest_pose returned true");
            Console.WriteLine("  [ok] rest pose evaluated");

            // --- bone count ---
            nint boneCount = Native.mmd_runtime_instance_bone_count(instance);
            AssertEqual((nint)2, boneCount, "bone_count == 2");
            Console.WriteLine("  [ok] bone_count == 2");

            // --- world matrix f32 len ---
            nint worldLen = Native.mmd_runtime_instance_world_matrix_f32_len(instance);
            AssertEqual((nint)32, worldLen, "world_matrix_f32_len == 32");
            Console.WriteLine("  [ok] world_matrix_f32_len == 32");

            // --- pointer-view world matrices (non-null) ---
            nint worldPtr = Native.mmd_runtime_instance_world_matrices(instance);
            Assert(worldPtr != nint.Zero, "world_matrices ptr != null");
            Console.WriteLine("  [ok] world_matrices ptr != null");

            // --- pointer-view skinning matrices (non-null) ---
            nint skinPtr = Native.mmd_runtime_instance_skinning_matrices(instance);
            Assert(skinPtr != nint.Zero, "skinning_matrices ptr != null");
            Console.WriteLine("  [ok] skinning_matrices ptr != null");

            // --- verify world matrix translation values from pointer view ---
            float[] worldFromPtr = new float[32];
            Marshal.Copy(worldPtr, worldFromPtr, 0, 32);
            AssertEqual(1.0f, worldFromPtr[12], "bone0 world tx (ptr)");
            AssertEqual(1.0f, worldFromPtr[28], "bone1 world tx (ptr)");
            AssertEqual(2.0f, worldFromPtr[29], "bone1 world ty (ptr)");
            Console.WriteLine("  [ok] pointer-view world matrix translations match Rust test");

            // --- copyWorldMatrices and compare with pointer view ---
            float[] worldCopy = new float[32];
            byte copyOk = Native.mmd_runtime_instance_copy_world_matrices(instance, worldCopy, (nint)worldCopy.Length);
            Assert(copyOk != 0, "copy_world_matrices returned true");
            for (int i = 0; i < 32; i++)
                AssertEqual(worldFromPtr[i], worldCopy[i], $"world matrix [{i}] matches copy");
            Console.WriteLine("  [ok] copyWorldMatrices matches pointer view");

            // --- copySkinningMatrices and compare with pointer view ---
            nint skinLen = Native.mmd_runtime_instance_skinning_matrix_f32_len(instance);
            AssertEqual((nint)32, skinLen, "skinning_matrix_f32_len == 32");
            Console.WriteLine("  [ok] skinning_matrix_f32_len == 32");

            float[] skinFromPtr = new float[32];
            Marshal.Copy(skinPtr, skinFromPtr, 0, 32);

            float[] skinCopy = new float[32];
            byte skinCopyOk = Native.mmd_runtime_instance_copy_skinning_matrices(instance, skinCopy, (nint)skinCopy.Length);
            Assert(skinCopyOk != 0, "copy_skinning_matrices returned true");
            for (int i = 0; i < 32; i++)
                AssertEqual(skinFromPtr[i], skinCopy[i], $"skinning matrix [{i}] matches copy");
            Console.WriteLine("  [ok] copySkinningMatrices matches pointer view");

            Console.WriteLine("  [ok] all assertions passed");

            // --- clip evaluation (frame 30 of a 60-frame animation) ---
            var boneTracks = new Native.MmdRuntimeFfiBoneTrack[1]
            {
                new()
                {
                    bone_index = 0,
                    keyframe_offset = 0,
                    keyframe_count = 2,
                },
            };
            var boneKeyframes = new Native.MmdRuntimeFfiBoneKeyframe[2]
            {
                new()
                {
                    frame = 0,
                    position_xyz = [0f, 0f, 0f],
                    rotation_xyzw = [0f, 0f, 0f, 1f],
                },
                new()
                {
                    frame = 60,
                    position_xyz = [2f, 0f, 0f],
                    rotation_xyzw = [0f, 0f, 0f, 1f],
                },
            };
            var morphTracks = new Native.MmdRuntimeFfiMorphTrack[1]
            {
                new()
                {
                    morph_index = 0,
                    keyframe_offset = 0,
                    keyframe_count = 2,
                },
            };
            var morphKeyframes = new Native.MmdRuntimeFfiMorphKeyframe[2]
            {
                new() { frame = 0, weight = 0f },
                new() { frame = 60, weight = 1f },
            };
            var propertyKeyframes = new Native.MmdRuntimeFfiPropertyKeyframe[2]
            {
                new()
                {
                    frame = 0,
                    ik_enabled_offset = 0,
                    ik_enabled_count = 1,
                },
                new()
                {
                    frame = 30,
                    ik_enabled_offset = 1,
                    ik_enabled_count = 1,
                },
            };
            byte[] propertyIkEnabled = [1, 0];

            clip = Native.mmd_runtime_clip_create(
                boneTracks, (nint)1,
                boneKeyframes, (nint)2,
                morphTracks, (nint)1,
                morphKeyframes, (nint)2,
                propertyKeyframes, (nint)2,
                propertyIkEnabled, (nint)2);
            Assert(clip != nint.Zero, "clip created");
            Console.WriteLine("  [ok] clip created");
            byte rangeOk = Native.mmd_runtime_clip_frame_range(
                clip,
                out uint firstFrame,
                out uint lastFrame);
            Assert(rangeOk != 0, "clip frame range returned true");
            AssertEqual(0u, firstFrame, "clip first frame");
            AssertEqual(60u, lastFrame, "clip last frame");
            Console.WriteLine("  [ok] clip frame range == 0..60");

            byte clipOk = Native.mmd_runtime_instance_evaluate_clip_frame(instance, clip, 30f);
            Assert(clipOk != 0, "evaluate_clip_frame returned true");
            Console.WriteLine("  [ok] clip evaluated at frame 30");

            // --- world matrix bone 0 x = rest(1.0) + clip_offset(1.0) = 2.0 (lerp 0→2) ---
            float[] clipWorld = new float[32];
            byte cwOk = Native.mmd_runtime_instance_copy_world_matrices(
                instance,
                clipWorld,
                (nint)32);
            Assert(cwOk != 0, "copy_world_matrices after clip");
            AssertEqual(2.0f, clipWorld[12], "bone0 world tx at frame 30");
            Console.WriteLine("  [ok] bone0 world tx == 2.0");

            // --- morph weight should be 0.5 (lerp from 0→1 over 60 frames) ---
            nint mwLen = Native.mmd_runtime_instance_morph_weight_len(instance);
            AssertEqual((nint)1, mwLen, "morph_weight_len == 1");
            Console.WriteLine("  [ok] morph_weight_len == 1");

            float[] mw = new float[1];
            byte mwOk = Native.mmd_runtime_instance_copy_morph_weights(instance, mw, (nint)1);
            Assert(mwOk != 0, "copy_morph_weights returned true");
            AssertEqual(0.5f, mw[0], "morph weight at frame 30");
            Console.WriteLine("  [ok] morph weight == 0.5");

            // --- IK enabled should be 0 (keyframe at frame 30 sets it to 0) ---
            nint ikLen = Native.mmd_runtime_instance_ik_enabled_len(instance);
            AssertEqual((nint)1, ikLen, "ik_enabled_len == 1");
            Console.WriteLine("  [ok] ik_enabled_len == 1");

            byte[] ik = new byte[1];
            byte ikOk = Native.mmd_runtime_instance_copy_ik_enabled(instance, ik, (nint)1);
            Assert(ikOk != 0, "copy_ik_enabled returned true");
            AssertEqual((byte)0, ik[0], "IK enabled at frame 30");
            Console.WriteLine("  [ok] IK enabled == 0");

            // --- direct pointer morph weight view ---
            nint morphPtr = Native.mmd_runtime_instance_morph_weights(instance);
            Assert(morphPtr != nint.Zero, "morph_weights ptr != null");
            float[] morphFromPtr = new float[1];
            Marshal.Copy(morphPtr, morphFromPtr, 0, 1);
            AssertEqual(mw[0], morphFromPtr[0], "morph weight direct ptr matches copy");
            Console.WriteLine("  [ok] direct morph_weights pointer matches copy API");

            // --- direct pointer IK enabled view ---
            nint ikPtr = Native.mmd_runtime_instance_ik_enabled(instance);
            Assert(ikPtr != nint.Zero, "ik_enabled ptr != null");
            byte[] ikFromPtr = new byte[1];
            Marshal.Copy(ikPtr, ikFromPtr, 0, 1);
            AssertEqual(ik[0], ikFromPtr[0], "IK enabled direct ptr matches copy");
            Console.WriteLine("  [ok] direct ik_enabled pointer matches copy API");

            Console.WriteLine("  [ok] all clip assertions passed");
        }
        finally
        {
            if (clip != nint.Zero)
                Native.mmd_runtime_clip_free(clip);
            if (instance != nint.Zero)
                Native.mmd_runtime_instance_free(instance);
            if (model != nint.Zero)
                Native.mmd_runtime_model_free(model);
        }
    }
}

public static class EntryPoint
{
    public static int Main()
    {
        try
        {
            Console.WriteLine("=== mmd-anim-ffi C# smoke ===");
            Smoke.Run();
            Console.WriteLine("=== PASS ===");
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"FAIL: {ex.Message}");
            return 1;
        }
    }
}
