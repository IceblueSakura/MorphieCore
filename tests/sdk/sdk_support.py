"""Pinned, isolated SDK client and optimization-independent scenario checks."""
import ipaddress
from urllib.parse import urlsplit
import openai


class SdkCheckFailure(RuntimeError):
    """A synthetic SDK result failed its independent oracle."""


def check(condition, message="SDK contract failed"):
    if not condition:
        raise SdkCheckFailure(message)


def client_for(base_url: str) -> openai.OpenAI:
    """Reject non-loopback targets and private client defaults before making requests."""
    if openai.__version__ != "3.24.0":
        raise RuntimeError("expected pinned openai==3.24.0")
    parsed = urlsplit(base_url)
    if (parsed.scheme != "http" or not parsed.hostname or not parsed.port
            or not ipaddress.ip_address(parsed.hostname).is_loopback
            or parsed.path != "/v1" or parsed.query or parsed.fragment
            or parsed.username or parsed.password):
        raise ValueError("only a literal loopback /v1 listener is allowed")
    return openai.OpenAI(
        api_key="synthetic-local-token", base_url=base_url, max_retries=0,
        timeout=8.0, organization="", project="", _strict_response_validation=True,
        http_client=openai.DefaultHttpxClient(trust_env=False, follow_redirects=False),
    )
