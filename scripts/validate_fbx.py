"""Validate an FBX file exported by mmd-anim using Maya's FBX importer.

Usage:
    & "C:\Program Files\Autodesk\Maya2026\bin\mayapy.exe" scripts\validate_fbx.py <fbx_path> [--json]

Loads the FBX, inspects mesh/material/skeleton structure, and prints a
validation report. With --json the report is machine-readable JSON.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def init_maya():
    import maya.standalone
    maya.standalone.initialize(name="python")
    import maya.cmds as cmds
    cmds.loadPlugin("fbxmaya", quiet=True)
    return cmds


def validate(fbx_path: str, cmds) -> dict:
    cmds.file(new=True, force=True)
    cmds.file(fbx_path, i=True, type="FBX", ignoreVersion=True,
              mergeNamespacesOnClash=False, options="fbx", pr=True)

    report: dict = {"file": fbx_path, "errors": [], "warnings": []}

    # -- Meshes --
    meshes = cmds.ls(type="mesh", long=True) or []
    transforms = []
    mesh_details = []
    for m in meshes:
        parent = cmds.listRelatives(m, parent=True, fullPath=True)
        t = parent[0] if parent else m
        if t not in transforms:
            transforms.append(t)
        vcount = cmds.polyEvaluate(m, vertex=True)
        fcount = cmds.polyEvaluate(m, face=True)
        tcount = cmds.polyEvaluate(m, triangle=True)
        mesh_details.append({
            "shape": m.split("|")[-1],
            "transform": t.split("|")[-1],
            "vertices": vcount,
            "faces": fcount,
            "triangles": tcount,
        })
    report["meshes"] = mesh_details
    report["meshCount"] = len(meshes)
    report["totalVertices"] = sum(d["vertices"] for d in mesh_details)
    report["totalFaces"] = sum(d["faces"] for d in mesh_details)

    # -- Materials --
    shading_engines = cmds.ls(type="shadingEngine") or []
    skip = {"initialShadingGroup", "initialParticleSE"}
    materials = []
    for se in shading_engines:
        if se in skip:
            continue
        mat_connections = cmds.listConnections(se + ".surfaceShader") or []
        mat_name = mat_connections[0] if mat_connections else se
        mat_type = cmds.nodeType(mat_name) if mat_connections else "unknown"
        materials.append({"name": mat_name, "type": mat_type, "shadingEngine": se})
    report["materials"] = materials
    report["materialCount"] = len(materials)

    # -- Skeleton / Joints --
    joints = cmds.ls(type="joint", long=True) or []
    joint_details = []
    for j in joints:
        parent = cmds.listRelatives(j, parent=True, fullPath=True)
        parent_name = parent[0].split("|")[-1] if parent else None
        pos = cmds.xform(j, q=True, ws=True, t=True)
        rot = cmds.xform(j, q=True, ws=True, ro=True)
        joint_details.append({
            "name": j.split("|")[-1],
            "parent": parent_name,
            "worldPosition": [round(v, 6) for v in pos],
            "worldRotation": [round(v, 6) for v in rot],
        })
    report["joints"] = joint_details
    report["jointCount"] = len(joints)

    # -- Blend Shapes --
    blend_shapes = cmds.ls(type="blendShape") or []
    bs_details = []
    for bs in blend_shapes:
        targets = cmds.listAttr(bs + ".weight", multi=True) or []
        bs_details.append({"name": bs, "targetCount": len(targets), "targets": targets})
    report["blendShapes"] = bs_details
    report["blendShapeCount"] = len(blend_shapes)

    # -- Animation --
    anim_curves = cmds.ls(type="animCurve") or []
    report["animCurveCount"] = len(anim_curves)
    if anim_curves:
        min_time = cmds.playbackOptions(q=True, min=True)
        max_time = cmds.playbackOptions(q=True, max=True)
        report["animRange"] = [min_time, max_time]

    # -- Scene hierarchy (top-level only) --
    top_nodes = cmds.ls(assemblies=True, long=True) or []
    default_nodes = {"persp", "top", "front", "side"}
    scene_roots = [n for n in top_nodes if n.strip("|") not in default_nodes]
    report["sceneRoots"] = [n.strip("|") for n in scene_roots]

    # -- Basic sanity checks --
    if not meshes:
        report["errors"].append("No mesh found in FBX")
    if report["totalVertices"] == 0:
        report["errors"].append("Mesh has 0 vertices")
    if report["totalFaces"] == 0:
        report["errors"].append("Mesh has 0 faces")

    report["valid"] = len(report["errors"]) == 0
    return report


def main():
    args = sys.argv[1:]
    if not args or args[0] in ("-h", "--help"):
        print(__doc__.strip())
        sys.exit(0)

    fbx_path = str(Path(args[0]).resolve())
    use_json = "--json" in args

    cmds = init_maya()
    report = validate(fbx_path, cmds)

    if use_json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
    else:
        status = "PASS" if report["valid"] else "FAIL"
        print(f"[{status}] {report['file']}")
        print(f"  meshes:      {report['meshCount']}")
        print(f"  vertices:    {report['totalVertices']}")
        print(f"  faces:       {report['totalFaces']}")
        print(f"  materials:   {report['materialCount']}")
        for m in report["materials"]:
            print(f"    - {m['name']} ({m['type']})")
        print(f"  joints:      {report['jointCount']}")
        print(f"  blendShapes: {report['blendShapeCount']}")
        print(f"  animCurves:  {report['animCurveCount']}")
        print(f"  sceneRoots:  {report['sceneRoots']}")
        if report["errors"]:
            for e in report["errors"]:
                print(f"  ERROR: {e}")
        if report["warnings"]:
            for w in report["warnings"]:
                print(f"  WARN:  {w}")

    sys.exit(0 if report["valid"] else 1)


if __name__ == "__main__":
    main()
