# OCLA-Umbau-Ziel

Dieses Dokument ist die Fortschrittstabelle für die OCLA-Phasen und trennt
fertige Verträge von noch fehlender Produktions-Adoption.

## Fortschritt pro P-Phase

| Phase | Ziel | Stand auf `main` | Evidenz / nächster Schritt |
| --- | --- | --- | --- |
| P0 | IST-Hygiene | Erledigt | `e379c9db0`; Grundlagen bereinigt. |
| P1 | Foundation, Contracts, Ledger Evidence, Lineage | Erledigt | `#1053`, `79290e63d`. |
| P2 | OclaBus Event-Backbone | Erledigt | `1029229a1`; globaler Bus mit bounded/no-op-Modus. |
| P3 | Builtin-Traits | In Arbeit: ~85 % | 14 Traits und Implementierungen vorhanden; Boundary-Härtung und SSOT-Sync werden noch abgeschlossen. |
| P4 | Trait-Adoption in Runtime | In Arbeit: ~85 % (12/14) | Runde-2-Pfade für Gateway, Scheduler, Tuner, Experimente, Router und Savings; `IntentClassifier` und `ResponseOptimizer` offen. |
| P5 | Unified Ledger + Binary Separation | Ausstehend | Unified Evidence-/Lineage-Ledger bauen und OSS-/Private-Binaries sauber trennen. |
| P6 | Separater OCLA-Meilenstein | Nicht belegt | Kein eigenständiger P6-OCLA-Commit im aktuellen Verlauf; Scope klären. |
| P7 | Wire Protocol, SDKs, gRPC, Contract Suite | Erledigt | `f5c447a63`; öffentliche OCLA-v1-Verifikation vorhanden. |
| P8 | Intent-/Model-Router | Implementiert, Adoption teilweise offen | `6b109c739`, `370495651`; ModelRouter verdrahtet, IntentClassifier offen. |
| P9 | Response Optimizer | Implementiert, Adoption offen | `6136ca554`; Builtin vorhanden, produktiver Registry-Aufruf fehlt. |
| P10 | Separater OCLA-Meilenstein | Nicht belegt | Kein eigenständiger P10-OCLA-Commit im aktuellen Verlauf; Scope klären. |
| P11 | Agent Gateway und Deployment Surface | Vertrag/Module erledigt, Adoption offen | `40f3f97a1`; `BuiltinAgentGateway` noch nicht am A2A-Ingress verdrahtet. |

## Produktions-Adoption: aktueller Zähler

| Verdrahtet | Gehärtet | Offen | Gesamt |
| ---: | ---: | ---: |
| 11 | 1 | 2 | 14 |

Verdrahtete Pfade: `AgentGateway`, `ConfigTuner`, `ConnectorScheduler`,
`EfficiencyAnalyzer`, `ExperimentRunner`, `MetricsExporter`, `ModelRouter`,
`ObservationHook`, `OutcomeTracker`, `SavingsLedger`, `UsageSink`.
Gehärtet: `CompressionProvider` (zusätzlich verdrahtet).
Offen: `IntentClassifier`, `ResponseOptimizer`.

## Gemergte OCLA-Änderungen

- `#1053` — P1 Foundation.
- `#1065` — Trait-Adoption-Grundlage: 14 Builtins plus Registry.
- `#1070` — UsageSink und EfficiencyAnalyzer produktiv.
- `#1071` — ObservationHook produktiv.
- `#1073` — OutcomeTracker produktiv.
- `#1075` — CompressionContentPort mit PathJail/BLAKE3.
- `#1076` — echte CompressionProvider-Kompression.
- `#1083` — fail-closed Provider und TOCTOU-Härtung.
- `#1092` — Projektwurzel und Runtime-Callsite korrigiert.
- `#1093` — MetricsExporter produktiv.
- `2127b0f42` — AgentGateway am Agent-Bus-Routing.
- `923cc59bf` — ConnectorScheduler in der Provider-Pipeline.
- `a9616d039` — ConfigTuner an die Adaptive-Mode-Policy.
- `0326e5079` — ExperimentRunner an die Routing-Evaluation.
- `370495651` — ModelRouter an die Proxy-Modellselektion.
- `698844afc` — SavingsLedger an verifizierte Events.
- `5394aa6e3` — Fail-closed-Gates und Compression-Härtung.

## Abschlusskriterien

Der Umbau ist erst abgeschlossen, wenn alle 14 Builtins einen belegten
Produktionsaufrufer besitzen, P5 den Unified Ledger und die Binary Separation
liefert, jeder Pfad Fehler fail-closed behandelt und die Legacy-Pipelines weder
doppelt buchen noch durch OCLA-Feedback verändert werden. Der OSS/Private-
Boundary-Audit muss vor dem Merge sauber sein.
