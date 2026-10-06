"""Independent strict Models consumption against the synthetic Gateway only."""
import openai
from sdk_support import check


def check_models(client: openai.OpenAI) -> int:
    """Check standard list/retrieve, non-pagination, auth and deletion failure."""
    expected = [
        {"id": "cross-model", "object": "model", "created": 7, "owned_by": "Synthetic Developer"},
        {"id": "gpt-image-2.5-flare", "object": "model", "created": 1788825600, "owned_by": "OpenAI"},
        {"id": "no-files-model", "object": "model", "created": 7, "owned_by": "Synthetic Developer"},
        {"id": "public-image", "object": "model", "created": 8, "owned_by": "Synthetic Image Developer"},
        {"id": "public-model", "object": "model", "created": 7, "owned_by": "Synthetic Developer"},
        {"id": "public-speech", "object": "model", "created": 9, "owned_by": "Synthetic Speech Developer"},
        {"id": "qwen-audio-3.0-asr-flash", "object": "model", "created": 1785369600, "owned_by": "Alibaba"},
        {"id": "qwen-audio-3.0-tts-flash", "object": "model", "created": 1784592000, "owned_by": "Alibaba"},
        {"id": "qwen-audio-3.0-tts-plus", "object": "model", "created": 1784592000, "owned_by": "Alibaba"},
    ]
    page = client.models.list()
    requests = 1
    check(page.to_dict() == {"object": "list", "data": expected})
    check(not page.has_next_page())
    check([model.to_dict() for model in page] == expected)
    # One punctuated label exercises retrieval; the list above checks every task's metadata.
    model = next(model for model in expected if model["id"] == "qwen-audio-3.0-asr-flash")
    requests += 1
    check(client.models.retrieve(model["id"]).to_dict() == model)
    for method, name, status, code in [
        (client.models.retrieve, "unknown", 404, "model_not_found"),
        (client.models.delete, "unknown", 404, "model_not_found"),
        (client.models.delete, "public-model", 403, "model_deletion_forbidden"),
    ]:
        requests += 1
        try:
            method(name)
        except openai.APIStatusError as error:
            check(error.status_code == status)
            check(error.response.json()["error"]["code"] == code)
        else:
            check(False, "unknown/deleted model must not appear successful")
    try:
        requests += 1
        client.with_options(api_key="synthetic-wrong-token").models.list()
    except openai.AuthenticationError as error:
        check(error.status_code == 401)
    else:
        check(False, "discovery must require the Gateway's client key")
    requests += 1
    check(client.models.list().to_dict() == page.to_dict(), "DELETE must not mutate activation")
    return requests
