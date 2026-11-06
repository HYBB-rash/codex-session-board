import io
import json
import sys
import unittest
from contextlib import redirect_stdout
from pathlib import Path


sys.path.insert(0, str(Path.cwd().resolve()))


class RecordedResult(unittest.TextTestResult):
    def __init__(self, *arguments):
        super().__init__(*arguments)
        self.records = []

    def record(self, test, status, detail=""):
        self.records.append({"id": test.id(), "status": status, "detail": detail})

    def addSuccess(self, test):
        super().addSuccess(test)
        self.record(test, "pass")

    def addFailure(self, test, error):
        super().addFailure(test, error)
        self.record(test, "fail", self._exc_info_to_string(error, test))

    def addError(self, test, error):
        super().addError(test, error)
        self.record(test, "error", self._exc_info_to_string(error, test))

    def addSkip(self, test, reason):
        super().addSkip(test, reason)
        self.record(test, "skip", reason)

    def addExpectedFailure(self, test, error):
        super().addExpectedFailure(test, error)
        self.record(test, "fail", "Expected failure is still unmet acceptance.")

    def addUnexpectedSuccess(self, test):
        super().addUnexpectedSuccess(test)
        self.record(test, "fail", "Unexpected success requires test repair.")

    def addSubTest(self, test, subtest, error):
        super().addSubTest(test, subtest, error)
        if error is not None:
            self.record(test, "fail", self._exc_info_to_string(error, test))


def test_identifiers(suite):
    for test in suite:
        if isinstance(test, unittest.TestSuite):
            yield from test_identifiers(test)
        else:
            yield test.id()


def run(request):
    if request.get("protocol") != 1:
        raise ValueError("Unsupported test adapter protocol")
    requested = request["tests"]
    if (
        not isinstance(requested, list) or not requested
        or not all(isinstance(name, str) and name for name in requested)
        or len(requested) != len(set(requested))
    ):
        raise ValueError("Expected distinct test identifiers")
    captured = io.StringIO()
    with redirect_stdout(captured):
        inventory = list(test_identifiers(unittest.defaultTestLoader.discover(
            str(Path.cwd().resolve())
        )))
        if len(inventory) != len(set(inventory)) or set(inventory) != set(requested):
            raise ValueError(
                "Test inventory differs from bindings; missing or unbound checks: "
                + repr(sorted(set(inventory).symmetric_difference(requested)))
            )
        suite = unittest.TestSuite(
            unittest.defaultTestLoader.loadTestsFromName(name) for name in requested
        )
        result = unittest.TextTestRunner(
            stream=sys.stderr, verbosity=1, resultclass=RecordedResult
        ).run(suite)
    if captured.getvalue():
        print(captured.getvalue(), file=sys.stderr, end="")
    records = {}
    for item in result.records:
        if item["id"] not in records or item["status"] != "pass":
            records[item["id"]] = item
    return {"protocol": 1, "results": list(records.values())}


if __name__ == "__main__":
    try:
        print(json.dumps(run(json.load(sys.stdin)), ensure_ascii=False))
    except Exception as error:
        print(json.dumps({"protocol": 1, "error": str(error)}))
        sys.exit(1)
