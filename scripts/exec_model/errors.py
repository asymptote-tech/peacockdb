"""Error types for the execution-model prototype."""


class PlanError(Exception):
    """A plan is structurally invalid: arity, partition topology, batch layout."""


class DriverError(Exception):
    """The driver reached a state a valid plan cannot produce."""


class ResidentBudgetExceeded(Exception):
    """The accounted resident set crossed the budget; the query fails cleanly."""


class EnginePlanFormatError(Exception):
    """An engine plan golden holds text this reader was not taught; the message names where."""


class StatsError(Exception):
    """Statistics the estimator needs are missing or stale; the message says how to regenerate."""
