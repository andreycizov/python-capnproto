import os
import unittest
from capnproto import wrapper


class TestHello(unittest.TestCase):

    def setUp(self) -> None:
        super().setUp()

    def _get(self):
        filename = os.path.join(os.path.split(__file__)[0], 'test.capnp')

        return wrapper.compile(filename)

    def test_hello1(self):
        for x in self._get().id:
            print(x)
            print(x.__repr__())
        self.assertEqual(wrapper.compile(filename).id, 'a')

    def test_hello2(self):
        def run(item, d=0):
            for x in item.children():
                print('\t'*d, x)
                run(getattr(item, x), d=d + 1)

        for root in self._get().id:
            run(root)


        self.assertEqual(wrapper.compile(filename).id, 'a')
