# Ordinary array iterator stepping diagnostics

Source f9760f8 versus 3954a8a, one instruction-count capture per fixed input. Declaration -4.90%, parameters -3.63%, rest -4.83%; three custom-iterator cases increase by 0.11–0.21%. All adverse observations are retained. Native speed remains unqualified. All-target tests: 930 passed, 26 ignored; Clippy passes. Focused strict Miri passes for builtin_array_steps_keep_live_length_and_observable_element_access, covering live growth/shrink, completed iterators, inherited/own accessors and Proxy Get order in Ast and Auto.
