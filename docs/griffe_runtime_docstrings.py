"""Merge PyO3 runtime docstrings into Griffe's statically loaded stubs."""

import re
from inspect import cleandoc

import griffe

_DISPLAY_ALIASES = {
    "_Buffer": "bytes | bytearray | memoryview",
    "_Event": (
        "Request | InformationalResponse | Response | Data | EndOfMessage | "
        "ConnectionClosed"
    ),
    "_HeaderInput": (
        "Iterable[tuple[str | bytes | bytearray | memoryview, "
        "str | bytes | bytearray | memoryview]]"
    ),
    "_Headers": "tuple[tuple[bytes, bytes], ...]",
}


def _display_annotation(annotation: str | griffe.Expr | None) -> str | None:
    if annotation is None:
        return None
    value = str(annotation)
    for name, replacement in _DISPLAY_ALIASES.items():
        value = re.sub(rf"\b{re.escape(name)}\b", replacement, value)
    return value


def _expand_private_aliases(obj: griffe.Object) -> None:
    if obj.is_function:
        for parameter in obj.parameters:
            parameter.annotation = _display_annotation(parameter.annotation)
        obj.returns = _display_annotation(obj.returns)
    elif obj.is_attribute:
        obj.annotation = _display_annotation(obj.annotation)


def _copy_docstring(
    obj: griffe.Object,
    runtime_obj: object,
    loader: griffe.GriffeLoader,
) -> None:
    if obj.docstring:
        return
    runtime_docstring = getattr(runtime_obj, "__doc__", None)
    if runtime_docstring:
        obj.docstring = griffe.Docstring(
            cleandoc(runtime_docstring),
            parent=obj,
            parser=loader.docstring_parser,
            parser_options=loader.docstring_options,
        )


class RuntimeDocstrings(griffe.Extension):
    """Preserve stub signatures while adding compiled-object documentation."""

    def on_package(
        self,
        *,
        pkg: griffe.Module,
        loader: griffe.GriffeLoader,
        **kwargs: object,
    ) -> None:
        if pkg.path != "h11r":
            return

        runtime_package = griffe.dynamic_import(pkg.path)
        for name in pkg.exports or ():
            alias = pkg.members.get(name)
            if alias is None or not alias.is_alias or not alias.target.is_class:
                continue

            target = alias.target
            runtime_class = getattr(runtime_package, name)
            _copy_docstring(target, runtime_class, loader)
            for member in target.members.values():
                _expand_private_aliases(member)
                if member.name.startswith("_") or not (
                    member.is_function or "property" in member.labels
                ):
                    continue
                runtime_member = getattr(runtime_class, member.name, None)
                if runtime_member is not None:
                    _copy_docstring(member, runtime_member, loader)


if __name__ == "__main__":
    package = griffe.load(
        "h11r",
        search_paths=["crates/h11r-python/python"],
        extensions=griffe.Extensions(RuntimeDocstrings()),
        docstring_parser="google",
    )
    connection = package.members["Connection"].target
    send_data_parts = connection.members["send_data_parts"]
    send_request = connection.members["send_request"]
    request_method = package.members["Request"].target.members["method"]

    assert send_request.docstring.value.startswith("Serialize a request head")
    assert "bytearray" in str(send_request.signature())
    assert "_Buffer" not in str(send_request.signature())
    assert send_data_parts.docstring.value.startswith(
        "Return `(prefix, original_object, suffix)`"
    )
    assert str(send_data_parts.signature()).count("_DataT") == 2
    assert request_method.docstring.value.startswith(
        "The case-sensitive request method"
    )
