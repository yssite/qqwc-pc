import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;
import java.io.*;
import java.util.*;

public class DecompileAt extends GhidraScript {
    public void run() throws Exception {
        DecompInterface ifc = new DecompInterface();
        ifc.setOptions(new DecompileOptions());
        ifc.openProgram(currentProgram);
        FunctionManager fm = currentProgram.getFunctionManager();
        String list = System.getenv("DECOMP_ADDRS");
        String outDir = System.getenv("DECOMP_OUT");
        if (outDir == null) outDir = "decomp";
        PrintWriter pw = new PrintWriter(new FileWriter(new File(outDir, "jni_functions.c")));
        AddressSpace sp = currentProgram.getAddressFactory().getDefaultAddressSpace();
        List<Function> targets = new ArrayList<>();
        Set<Long> seen = new HashSet<>();
        for (String s : list.split(",")) {
            long a = Long.parseLong(s.trim(), 16);
            Address ad = sp.getAddress(a);
            Function f = fm.getFunctionAt(ad);
            if (f == null) {
                disassemble(ad);
                try { f = createFunction(ad, "jni_" + Long.toHexString(a)); } catch (Exception e) { println("create fail " + s + " " + e); }
            }
            if (f != null && seen.add(f.getEntryPoint().getOffset())) targets.add(f);
        }
        // second pass: also decompile callees
        List<Function> extra = new ArrayList<>();
        for (Function f : new ArrayList<>(targets)) {
            for (Function c : f.getCalledFunctions(monitor)) {
                if (c != null && seen.add(c.getEntryPoint().getOffset())) extra.add(c);
            }
        }
        targets.addAll(extra);
        int ok = 0;
        for (Function f : targets) {
            try {
                DecompileResults res = ifc.decompileFunction(f, 90, monitor);
                if (res != null && res.decompileCompleted()) {
                    pw.println("/* ===== " + f.getName() + " @ " + f.getEntryPoint() + " ===== */");
                    pw.println(res.getDecompiledFunction().getC());
                    pw.println();
                    ok++;
                }
            } catch (Exception e) { pw.println("/* FAILED " + f.getName() + " " + e + " */"); }
        }
        pw.close();
        println("targets=" + targets.size() + " ok=" + ok);
    }
}
