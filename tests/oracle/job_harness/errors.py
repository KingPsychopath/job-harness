"""Typed local errors shared by the ledger and its generated views."""


class CareerError(ValueError):
    def __init__(self, message, code="invalid_state"):
        super().__init__(message)
        self.code = code
