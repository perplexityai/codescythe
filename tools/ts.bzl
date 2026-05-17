def ts_library(name, srcs, **kwargs):
    attrs = {
        "name": name,
        "srcs": srcs,
    }
    for attr in ["tags", "testonly", "visibility"]:
        if attr in kwargs:
            attrs[attr] = kwargs[attr]

    native.filegroup(**attrs)
