import unittest
from counter import increment

class CounterTest(unittest.TestCase):
    def test_increment(self):
        for value in (-1, 0, 5):
            self.assertEqual(increment(value), value + 2)
