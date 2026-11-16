import json
from pathlib import Path
import sys
from urllib.parse import urlsplit


def main(database):
    path = Path(database)
    data = json.loads(path.read_text())
    arguments = sys.argv[1:]
    position = arguments.index("github.com") + 1
    included = arguments[position] == "--include"
    endpoint = arguments[position + int(included)]
    endpoint = urlsplit(endpoint).path
    method = arguments[arguments.index("--method") + 1] if "--method" in arguments else "GET"
    fields = dict(arguments[index + 1].split("=", 1)
                  for index, value in enumerate(arguments) if value in {"-f", "-F"})
    prefix = "repos/owner/repo"
    if endpoint == prefix:
        value = {"full_name": "owner/repo"}
    elif endpoint == "user":
        value = {"login": "owner"}
    elif endpoint == prefix + "/pulls/7":
        value = data["pr"]
    elif endpoint.endswith(("/reviews", "/statuses")) or endpoint == prefix + "/pulls/7/comments":
        value = []
    elif endpoint.endswith("/check-runs"):
        value = {"check_runs": data["checks"]}
    elif endpoint == prefix + "/labels":
        if method == "POST":
            data["labels"].append(fields)
        value = data["labels"]
    elif endpoint == prefix + "/issues/7":
        value = {"labels": []}
    elif endpoint == prefix + "/issues/7/labels":
        value = []
    elif endpoint == prefix + "/issues/7/comments":
        if method == "POST":
            entry = {"id": len(data["comments"]) + 1, "body": fields["body"],
                     "user": {"login": "owner"}, "html_url": "https://github.com/fixture/comment"}
            data["comments"].append(entry)
            value = entry
        else:
            value = data["comments"]
    elif endpoint.startswith(prefix + "/issues/comments/"):
        value = next(entry for entry in data["comments"] if entry["id"] == int(endpoint.rsplit("/", 1)[1]))
    else:
        raise RuntimeError(f"Unexpected fixture request: {method} {endpoint}")
    data["calls"].append({"endpoint": endpoint, "method": method})
    path.write_text(json.dumps(data))
    if "--jq" in arguments:
        print(value["full_name"])
    else:
        if included:
            print("HTTP/2.0 200 OK\nContent-Type: application/json\n")
        print(json.dumps(value))
