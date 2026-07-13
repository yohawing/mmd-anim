# Vendored Bullet3 source

- Upstream: https://github.com/bulletphysics/bullet3
- Upstream version: `3.25-27-g63c4d67e3`
- Upstream commit: `63c4d67e337017f9d8b298c900e9aabdb69296e7`
- Imported modules: `LinearMath`, `BulletCollision`, `BulletDynamics`, plus
  their two top-level aggregate headers

This snapshot is compiled only when the crate's `native` feature is enabled.
The upstream zlib license is preserved in `LICENSE.txt`.

To test another Bullet3 checkout without changing the vendored snapshot, set
`MMD_ANIM_BULLET3_DIR` to the checkout root before building.
