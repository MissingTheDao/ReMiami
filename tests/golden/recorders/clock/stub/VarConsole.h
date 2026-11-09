#pragma once
struct CVarConsole { template <class... A> void Add(A...) {} };
static CVarConsole VarConsole;
