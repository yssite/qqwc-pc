import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import java.io.*;
import java.util.*;

public class DecompileAll extends GhidraScript {
    public void run() throws Exception {
        DecompInterface ifc = new DecompInterface();
        DecompileOptions opts = new DecompileOptions();
        ifc.setOptions(opts);
        ifc.toggleCCode(true);
        ifc.toggleSyntaxTree(true);
        ifc.openProgram(currentProgram);

        String outDir = System.getenv("DECOMP_OUT");
        if (outDir == null) outDir = "decomp";
        new File(outDir).mkdirs();
        PrintWriter pw = new PrintWriter(new FileWriter(new File(outDir, "all_functions.c")));

        FunctionManager fm = currentProgram.getFunctionManager();
        int n = 0, ok = 0;
        for (Function f : fm.getFunctions(true)) {
            n++;
            if (f.isThunk()) continue;
            try {
                DecompileResults res = ifc.decompileFunction(f, 60, monitor);
                if (res != null && res.decompileCompleted()) {
                    pw.println("/* ===== " + f.getName() + " @ " + f.getEntryPoint() + " ===== */");
                    pw.println(res.getDecompiledFunction().getC());
                    pw.println();
                    ok++;
                }
            } catch (Exception e) {
                pw.println("/* FAILED " + f.getName() + " : " + e + " */");
            }
        }
        pw.close();
        println("functions=" + n + " decompiled=" + ok);
    }
}
