"""Small shared validators for immutable Python bridge records."""

from __future__ import annotations

from collections.abc import Iterable


def fixed_tuple(value: Iterable[float], *, name: str, length: int) -> tuple[float, ...]:
    try:
        result = tuple(value)
    except TypeError as error:
        raise TypeError(f"{name} must be iterable") from error
    if len(result) != length:
        raise ValueError(f"{name} must contain exactly {length} values")
    return result


def flat_tuple(value: Iterable[float], *, name: str, stride: int) -> tuple[float, ...]:
    result = tuple(value)
    if len(result) % stride:
        raise ValueError(f"{name} length must be a multiple of {stride}")
    return result


def uint32(value: object, *, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{name} must be an integer")
    if not 0 <= value <= 0xFFFFFFFF:
        raise ValueError(f"{name} must fit uint32")
    return value


def nonnegative_size(value: object, *, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{name} must be an integer")
    if value < 0:
        raise ValueError(f"{name} must be non-negative")
    return value


def strict_bool(value: object, *, name: str) -> bool:
    if type(value) is not bool:
        raise TypeError(f"{name} must be bool")
    return value
