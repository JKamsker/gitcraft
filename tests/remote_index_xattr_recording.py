"""Pure recording gates for the embedded remote metadata reader.

Only selected function definitions from production AST are evaluated. Helper imports,
main, JSON input, resource limits, Git, native CDLL calls and actual xattrs never run.
Run: python3 tests/remote_index_xattr_recording.py -v
"""
import ast
import ctypes
from pathlib import Path
from types import SimpleNamespace
import unittest

SOURCE = Path(__file__).resolve().parents[1] / "src/git/file_comparison/remote_index.py"
LIMIT = 1024 * 1024


class Function:
    def __init__(self, call):
        self.call = call
        self.argtypes = None
        self.restype = None

    def __call__(self, *args):
        return self.call(*args)


class Native:
    def __init__(self, names, sizes, platform="linux"):
        self.names = names
        self.sizes = sizes
        self.platform = platform
        self.events = []
        self.allocations = []
        self.list_queries = 0
        self.list_reads = 0
        self.size_queries = {}
        self.list_size = None
        self.list_delta = 0
        self.final_names = None
        self.value_delta = 0
        self.query_growth = None
        self.flistxattr = Function(self.listing)
        self.fgetxattr = Function(self.get)

    def raw_names(self):
        names = self.final_names if self.list_reads and self.final_names is not None else self.names
        return b"".join(name + b"\0" for name in names)

    def listing(self, fd, buffer, size, *extra):
        assert fd == 37
        assert extra == ((0,) if self.platform == "darwin" else ())
        if buffer is None:
            self.list_queries += 1
            self.events.append(("list-size", size))
            return len(self.raw_names()) if self.list_size is None else self.list_size
        raw = self.raw_names()
        self.events.append(("list-read", size))
        if len(raw) <= size:
            ctypes.memmove(buffer, raw, len(raw))
        self.list_reads += 1
        return len(raw) + self.list_delta

    def get(self, fd, name, buffer, size, *extra):
        assert fd == 37
        assert extra == ((0, 0) if self.platform == "darwin" else ())
        if buffer is None:
            self.size_queries[name] = self.size_queries.get(name, 0) + 1
            self.events.append(("value-size", name))
            value = self.sizes[name]
            if self.query_growth == self.size_queries[name]:
                value += 1
            return value
        self.events.append(("value-read", name, size))
        # Model kernel's returned length/error without writing beyond the supplied
        # buffer, including ERANGE-like growth and concurrent shrinking fixtures.
        count = self.sizes[name] + self.value_delta
        if 0 < count <= size:
            ctypes.memset(buffer, 0x58, count)
        return count

    def cdll(self, name, use_errno):
        assert name == ("/usr/lib/libSystem.B.dylib" if self.platform == "darwin" else None)
        assert use_errno is True
        return self

    def allocate(self, size):
        self.allocations.append(size)
        assert 0 < size <= LIMIT, "unbounded native-buffer allocation requested"
        return ctypes.create_string_buffer(size)

    def load(self):
        tree = ast.parse(SOURCE.read_text())
        definitions = [node for node in tree.body if isinstance(node, ast.FunctionDef)
                       and node.name in {"xattrs", "access"}]
        assert len(definitions) == 2
        proxy = SimpleNamespace(**{name: getattr(ctypes, name) for name in (
            "c_int", "c_uint", "c_void_p", "c_char_p", "c_size_t", "c_ssize_t")},
            CDLL=self.cdll, create_string_buffer=self.allocate)
        scope = {"ctypes": proxy, "sys": SimpleNamespace(platform=self.platform),
                 "native_acl": lambda fd: None}
        exec(compile(ast.Module(body=definitions, type_ignores=[]), str(SOURCE), "exec"), scope)
        return lambda: scope["access"](37)

    def value_reads(self):
        return [event for event in self.events if event[0] == "value-read"]


class BoundedXattrs(unittest.TestCase):
    def test_count_overflow_refused_before_any_value_size_query_or_fetch(self):
        for platform in ("linux", "darwin"):
            with self.subTest(platform=platform):
                names = [f"user.n{i:04}".encode() for i in range(1025)]
                native = Native(names, dict.fromkeys(names, 65536), platform)
                with self.assertRaisesRegex(RuntimeError, "count exceeds 1024"):
                    native.load()()
                self.assertEqual(native.size_queries, {})
                self.assertEqual(native.value_reads(), [])

    def test_listing_size_refused_before_buffer_allocation(self):
        for invalid in (-1, LIMIT + 1):
            native = Native([], {})
            native.list_size = invalid
            with self.assertRaisesRegex(RuntimeError, "Cannot bound"):
                native.load()()
            self.assertEqual(native.allocations, [])
            self.assertEqual(native.value_reads(), [])

    def test_aggregate_budget_refused_before_any_value_allocation(self):
        for platform in ("linux", "darwin"):
            names = [f"user.n{i:02}".encode() for i in range(17)]
            native = Native(names, dict.fromkeys(names, 65536), platform)
            with self.assertRaisesRegex(RuntimeError, "exceed 1 MiB"):
                native.load()()
            self.assertEqual(len(native.size_queries), 16)  # names consume budget too
            self.assertEqual(native.value_reads(), [])
            self.assertEqual(native.allocations, [len(native.raw_names())])

    def test_individual_oversized_and_unreadable_values_refuse_before_allocation(self):
        for value_size in (LIMIT, -1):
            native = Native([b"user.value"], {b"user.value": value_size})
            with self.assertRaises(RuntimeError):
                native.load()()
            self.assertEqual(native.value_reads(), [])
            self.assertEqual(native.allocations, [len(native.raw_names())])

    def test_value_growth_shrink_and_error_during_read_refuse(self):
        for delta in (1, -1, -9):
            native = Native([b"user.value"], {b"user.value": 8})
            native.value_delta = delta
            with self.assertRaisesRegex(RuntimeError, "changed during reading"):
                native.load()()
            self.assertEqual(len(native.value_reads()), 1)
            self.assertEqual(native.allocations, [len(native.raw_names()), 8])

    def test_size_drift_before_allocation_and_after_read_refuses(self):
        for query_number in (2, 3):
            native = Native([b"user.value"], {b"user.value": 8})
            native.query_growth = query_number
            with self.assertRaisesRegex(RuntimeError, "changed"):
                native.load()()
            self.assertEqual(len(native.value_reads()), query_number - 2)

    def test_listing_drift_and_duplicate_names_refuse(self):
        native = Native([b"user.value"], {b"user.value": 8})
        native.list_delta = -1
        with self.assertRaisesRegex(RuntimeError, "changed during listing"):
            native.load()()
        self.assertEqual(native.value_reads(), [])
        native = Native([b"user.value", b"user.value"], {b"user.value": 8})
        with self.assertRaisesRegex(RuntimeError, "Malformed"):
            native.load()()
        self.assertEqual(native.size_queries, {})
        native = Native([b"user.value"], {b"user.value": 8})
        native.final_names = [b"user.other"]  # same byte count; content drift still refuses
        with self.assertRaisesRegex(RuntimeError, "names changed"):
            native.load()()

    def test_exact_budget_zero_values_and_native_signatures(self):
        for platform in ("linux", "darwin"):
            for sizes in ({b"user.value": LIMIT - len(b"user.value\0")},
                          {b"user.a": 0, b"user.b": 9}, {}):
                with self.subTest(platform=platform, sizes=list(sizes)):
                    native = Native(list(sizes), sizes, platform)
                    values, acl = native.load()()
                    self.assertEqual([(name, len(value)) for name, value in values], sorted(sizes.items()))
                    self.assertIsNone(acl)
                    self.assertEqual(native.list_queries, 2)
                    self.assertEqual(native.list_reads, 2)
                    self.assertEqual(native.flistxattr.argtypes,
                                     [ctypes.c_int, ctypes.c_void_p, ctypes.c_size_t]
                                     + ([ctypes.c_int] if platform == "darwin" else []))
                    self.assertEqual(native.fgetxattr.argtypes,
                                     [ctypes.c_int, ctypes.c_char_p, ctypes.c_void_p, ctypes.c_size_t]
                                     + ([ctypes.c_uint, ctypes.c_int] if platform == "darwin" else []))
                    self.assertIs(native.flistxattr.restype, ctypes.c_ssize_t)
                    self.assertIs(native.fgetxattr.restype, ctypes.c_ssize_t)


if __name__ == "__main__":
    unittest.main()
