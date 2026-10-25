def pytest_addoption(parser):
    parser.addoption("--slow", action="store_true", default=False, help="also run the slow parity tests")
