"""Deterministic one-page coding PDF; no user files, uploads or dependencies."""

import base64


def _pdf():
    """Encode a fixed Helvetica page with byte-accurate xref and stream lengths."""
    stream = (
        b"BT /F1 16 Tf 40 740 Td (Synthetic coding fixture) Tj "
        b"0 -30 Td (Build marker: BUILD-A17) Tj "
        b"0 -30 Td (Patch marker: PATCH-B29) Tj ET\n"
    )
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        b"<< /Length " + str(len(stream)).encode() + b" >>\nstream\n" + stream + b"endstream",
    ]
    result = bytearray(b"%PDF-1.4\n")
    offsets = []
    for number, value in enumerate(objects, 1):
        offsets.append(len(result))
        result.extend(f"{number} 0 obj\n".encode() + value + b"\nendobj\n")
    xref = len(result)
    result.extend(b"xref\n0 6\n0000000000 65535 f \n")
    for offset in offsets:
        result.extend(f"{offset:010d} 00000 n \n".encode())
    result.extend(
        b"trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n"
        + str(xref).encode() + b"\n%%EOF\n"
    )
    return bytes(result)


def file_url_history():
    """Public W3C test resource; see docs/references/multimodal-and-resources.md."""
    return [{"role":"user","content":[
        {"type":"input_text","text":"Read the public PDF and return only its exact visible text, without quotes or explanation."},
        {"type":"input_file","file_url":"https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf"},
    ]}]


def file_continuation_history():
    """Client-owned synthetic history, not a captured upstream transcript."""
    history = file_history()
    history.append({"type": "message", "role": "assistant", "status": "completed",
        "content": [{"type": "output_text", "text": "BUILD-A17",
                     "annotations": [], "logprobs": []}]})
    history.append({"role": "user", "content": "From the same document, return only the exact patch marker, without quotes or explanation."})
    return history


def file_history():
    """Answer requires the document; markers are not supplied in the prompt."""
    return [{"role": "user", "content": [
        {"type": "input_text", "text": "Inspect this synthetic build document for a coding task."},
        {"type": "input_file", "filename": "synthetic-build.pdf",
         "file_data": "data:application/pdf;base64," + base64.b64encode(_pdf()).decode("ascii")},
        {"type": "input_text", "text": "Return only the exact build marker, without quotes or explanation."},
    ]}]
