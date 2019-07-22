from setuptools import setup
from setuptools_rust import Binding, RustExtension

setup(
    name="capnproto",
    version="1.0",
    rust_extensions=[RustExtension("capnproto.wrapper", path="./rs/Cargo.toml", binding=Binding.PyO3)],
    package_dir={'': 'py'},
    packages=["capnproto"],
    # rust extensions are not zip safe, just like C-extensions.
    zip_safe=False,
)