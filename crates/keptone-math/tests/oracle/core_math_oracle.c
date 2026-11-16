// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).
//
//   core_math_oracle sinpi|cospi|exp2
//
// The correctly rounded functions of CORE-MATH (thirdparty/core-math, MIT) as a
// filter, for the tests of crates/keptone-math/tests/oracle.rs: binary64
// inputs on standard input, the function's results on standard output, both in
// the machine's byte order, until the input ends. The tests compile it with
// Clang, as C23 with every warning an error, link it with the three files of
// CORE-MATH it calls, and talk to it through pipes, so that the oracle needs no
// foreign-function interface in Rust.

#include <stdio.h>
#include <string.h>

#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

double cr_sinpi(double);
double cr_cospi(double);
double cr_exp2(double);

enum { CHUNK = 8192 };

int main(int argc, char **argv) {
  double (*function)(double) = nullptr;
  if (argc == 2 && strcmp(argv[1], "sinpi") == 0) {
    function = cr_sinpi;
  } else if (argc == 2 && strcmp(argv[1], "cospi") == 0) {
    function = cr_cospi;
  } else if (argc == 2 && strcmp(argv[1], "exp2") == 0) {
    function = cr_exp2;
  } else {
    fputs("usage: core_math_oracle sinpi|cospi|exp2\n", stderr);
    return 2;
  }
#ifdef _WIN32
  _setmode(_fileno(stdin), _O_BINARY);
  _setmode(_fileno(stdout), _O_BINARY);
#endif
  static double buffer[CHUNK];
  size_t count;
  while ((count = fread(buffer, sizeof buffer[0], CHUNK, stdin)) > 0) {
    for (size_t i = 0; i < count; i++) {
      buffer[i] = function(buffer[i]);
    }
    if (fwrite(buffer, sizeof buffer[0], count, stdout) != count) {
      return 1;
    }
  }
  return ferror(stdin) || fflush(stdout) != 0;
}
