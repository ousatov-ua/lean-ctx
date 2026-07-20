# OCLA Gateway-Integration — Handover

Stand: `main` bei `f658088c2` (Runde-2-Integration und Test-Fixes).

## Definition des Status

„Verdrahtet“ bedeutet hier: Der Builtin wird über
`OclaRegistry::global()` aus einem produktiven Laufzeitpfad aufgerufen.
„Gehärtet“ bezeichnet einen verdrahteten Pfad mit expliziten Boundary-,
Fail-closed- oder TOCTOU-Sicherungen. „Offen“ bezeichnet einen realen und
getesteten Builtin ohne produktiven Aufrufer; das ist kein Stub.

## Builtin-Inventar

| Builtin | Status | Aktueller Produktionspfad / Lücke |
| --- | --- | --- |
| `AgentGateway` | Verdrahtet | `tools/ctx_agent` nutzt Relay und Agent-Bus-Routing mit Envelope-Validierung. |
| `CompressionProvider` | Verdrahtet, gehärtet | Aggressive `ctx_read`-Kompression; ContentPort mit PathJail, BLAKE3-Referenz und fail-closed Gates. |
| `ConfigTuner` | Verdrahtet | Adaptive-Mode-Policy erzeugt deterministische Vorschläge mit Approval-Semantik. |
| `ConnectorScheduler` | Verdrahtet | Provider-Pipeline wählt verfügbare Connectoren bzw. Active-Inference-Fallback. |
| `EfficiencyAnalyzer` | Verdrahtet | `core/tool_lifecycle` berechnet Read-Density und ETPAO über den OCLA-Trait. |
| `ExperimentRunner` | Verdrahtet | Routing-Evaluation liefert deterministische Outcome- und Rollback-Referenzen. |
| `IntentClassifier` | Offen | Kandidatenbasierte Klassifikation vorhanden; Registry-Adoption fehlt noch. |
| `MetricsExporter` | Verdrahtet | `tools/server_metrics` exportiert pro MCP-Call ein begrenztes lokales Batch (`#1093`). |
| `ModelRouter` | Verdrahtet | OCLA-Routing ist an die Proxy-Modellselektion und Routing-Regeln angeschlossen. |
| `ObservationHook` | Verdrahtet | `tools/server_metrics` projiziert jeden MCP-Tool-Call als Observation. |
| `OutcomeTracker` | Verdrahtet | `tools/server_metrics` schreibt Accepted-/Quality-Ergebnis nach jedem MCP-Call. |
| `ResponseOptimizer` | Offen | Trait-Wrapper und deterministische Optimierung vorhanden; Legacy-Optimizer bleibt separat. |
| `SavingsLedger` | Verdrahtet | OCLA-Evidence wird in den verifizierten Core-Ledger projiziert; Unified Ledger bleibt P5. |
| `UsageSink` | Verdrahtet | `proxy/usage_meter` projiziert den finalisierten Provider-Turn in den OCLA-Sink. |

Damit sind 12 Builtins produktiv adoptiert (davon 1 zusätzlich gehärtet) und
2 Builtins für die nächste Adoptionsrunde offen; das entspricht rund 85 %.

## Gemergter Stand auf `main`

- `#1053`: P1 Foundation, Ledger Evidence, Contracts und Proxy Lineage.
- `#1065`: alle 14 Trait-Implementierungen und `OclaRegistry`.
- `#1070`: UsageSink- und EfficiencyAnalyzer-Produktionsaufrufe.
- `#1071`: ObservationHook am MCP-Tool-Call-Boundary.
- `#1073`: OutcomeTracker am MCP-Tool-Call-Boundary.
- `#1075`, `#1076`, `#1083`, `#1092`: sicherer ContentPort und echte,
  fail-closed CompressionProvider-Verdrahtung.
- `#1093`: MetricsExporter-Produktionsaufruf in `server_metrics`.
- `2127b0f42`: AgentGateway am Agent-Bus-Routing.
- `923cc59bf`: ConnectorScheduler in der Provider-Pipeline.
- `a9616d039`: ConfigTuner an die Adaptive-Mode-Policy.
- `0326e5079`: ExperimentRunner an die Routing-Evaluation.
- `370495651`: ModelRouter an die Proxy-Modellselektion.
- `698844afc`: SavingsLedger an verifizierte Events.
- `5394aa6e3`: Fail-closed-Gates und zusätzliche Compression-Härtung.

## Aktive Arbeit und nächste Schritte

Diese Dokumentation ist der SSOT-Sync von Agent 19. Der Review-/Merge-Agent
20 bleibt für Push und Merge zuständig; ungemergte Arbeitsstände gehören nicht
zum Stand auf `main`.

Nächste Schritte, in dieser Reihenfolge:

1. P5 „Unified Ledger“: OCLA-SavingsLedger und den verifizierten Core-Ledger
   zu einer Evidence-/Lineage-Pipeline ohne Doppelbuchungen zusammenführen.
2. „Binary Separation“: OSS-Kern und private/Enterprise-Deployment-Flächen
   trennen; Boundary-Audit vor dem Merge abschließen.
3. `IntentClassifier` und `ResponseOptimizer` an echte Registry-Aufrufer
   anschließen und pro Pfad Boundary-, Fehler- und Legacy-Feedback-Tests ergänzen.

## OSS/Private-Boundary-Audit

Der vorgeschriebene Suchlauf

```text
grep -rn 'enterprise\|Enterprise\|RBAC\|SSO\|multi.tenant\|value.gate' rust/src/ --include='*.rs'
```

liefert Treffer in `rust/src`. Beispiele sind Enterprise-markierte Kommentare
und Module rund um Deployment-Profile, Billing, Policy-Gate, SSO und Gateway-
Server. Das ist kein sauberer OSS-Audit: Der Befund wurde als Blocker an den
Agent-Bus gemeldet (Event `c3f76d6b`). Agent 20 bzw. der Boundary-Owner muss
vor dem Merge klären, welche Treffer entfernt oder in das Private-Repo
verschoben werden.

README.md und VISION.md enthalten keine widersprüchliche OCLA-Behauptung;
beide beschreiben weiterhin die lokale, provider-neutrale Architektur. Eine
Änderung dieser Dateien ist für den aktuellen Runtime-Stand nicht erforderlich.
