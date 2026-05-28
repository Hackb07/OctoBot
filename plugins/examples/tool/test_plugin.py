from plugin import run


def test_plugin_run():
    assert run({"ping": True})["status"] == "ok"
