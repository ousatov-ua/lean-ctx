from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Optional


class ReceiptOutcome(Enum):
    ACCEPTED = "accepted"
    REJECTED = "rejected"
    PARTIAL = "partial"
    UNKNOWN = "unknown"


class SensitivityLevel(Enum):
    PUBLIC = "public"
    INTERNAL = "internal"
    CONFIDENTIAL = "confidential"
    RESTRICTED = "restricted"


@dataclass
class QualitySignal:
    name: str
    value: float


@dataclass
class PlanEntry:
    object_id: str
    provider: str
    view: str
    tokens: int
    phi: float
    reason: str


@dataclass
class PlanBudget:
    total: int
    used: int


@dataclass
class ContextPlanV1:
    plan_id: str
    intent: str
    budget: PlanBudget
    selected: list[PlanEntry] = field(default_factory=list)
    excluded: list[dict] = field(default_factory=list)
    deferred: list[dict] = field(default_factory=list)
    provider_stats: dict[str, dict] = field(default_factory=dict)


@dataclass
class ContextReceiptV1:
    receipt_id: str
    plan_id: str
    delivered_tokens: int
    cache_hits: int
    cache_misses: int
    outcome: ReceiptOutcome
    quality_signals: list[QualitySignal] = field(default_factory=list)
    feedback_attribution: dict[str, float] = field(default_factory=dict)


@dataclass
class HealthResponse:
    status: str
    version: str


@dataclass
class TokenBalance:
    original_tokens: int
    materialized_tokens: int
    delivered_tokens: int
    provider_billed_tokens: int


@dataclass
class ContextPolicy:
    max_sensitivity: SensitivityLevel = SensitivityLevel.INTERNAL
    allowed_sources: Optional[list[str]] = None
    blocked_sources: list[str] = field(default_factory=list)
    budget_cap_tokens: Optional[int] = None
