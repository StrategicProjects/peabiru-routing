import java.io.*;
import java.util.*;
import org.geotools.geometry.DirectPosition2D;
import org.geotools.referencing.CRS;
import org.geotools.referencing.crs.DefaultGeographicCRS;
import org.geotools.referencing.operation.transform.ConcatenatedTransform;
import org.opengis.referencing.crs.CoordinateReferenceSystem;
import org.opengis.referencing.operation.MathTransform;

/** usage: Steps <points file: hex lon,lat per line> <out> <code>...
 *  out: per code a line "# code", then per point: final x y (as R5 does it, DirectPosition), then
 *  the output of each step (dims as the step gives them), or ERR <exception class>. */
public class Steps {
    static String h(double v) { return Long.toHexString(Double.doubleToRawLongBits(v)); }
    static void steps(MathTransform t, List<MathTransform> out) {
        if (t instanceof ConcatenatedTransform) {
            steps(((ConcatenatedTransform) t).transform1, out);
            steps(((ConcatenatedTransform) t).transform2, out);
        } else out.add(t);
    }
    public static void main(String[] a) throws Exception {
        List<double[]> pts = new ArrayList<>();
        try (BufferedReader r = new BufferedReader(new FileReader(a[0]))) {
            for (String l; (l = r.readLine()) != null; ) {
                if (l.isEmpty()) continue;
                int c = l.indexOf(',');
                pts.add(new double[] {Double.longBitsToDouble(Long.parseUnsignedLong(l.substring(0, c), 16)),
                        Double.longBitsToDouble(Long.parseUnsignedLong(l.substring(c + 1), 16))});
            }
        }
        boolean withSteps = !"0".equals(System.getenv("STEPS"));
        try (PrintWriter w = new PrintWriter(new BufferedWriter(new FileWriter(a[1]), 1 << 22))) {
            for (int k = 2; k < a.length; k++) {
                CoordinateReferenceSystem crs = CRS.decode("EPSG:" + a[k], true);
                MathTransform t = CRS.findMathTransform(DefaultGeographicCRS.WGS84, crs);
                List<MathTransform> s = new ArrayList<>();
                steps(t, s);
                w.println("# " + a[k] + " " + s.size());
                StringBuilder sb = new StringBuilder();
                for (double[] p : pts) {
                    sb.setLength(0);
                    try {
                        DirectPosition2D in = new DirectPosition2D(p[0], p[1]);
                        DirectPosition2D out = new DirectPosition2D();
                        t.transform(in, out);
                        sb.append(h(out.getOrdinate(0))).append(' ').append(h(out.getOrdinate(1)));
                    } catch (Exception e) {
                        sb.append("ERR ").append(e.getClass().getSimpleName());
                    }
                    if (withSteps) {
                        double[] cur = p.clone();
                        for (MathTransform m : s) {
                            double[] nxt = new double[m.getTargetDimensions()];
                            try {
                                m.transform(cur, 0, nxt, 0, 1);
                                sb.append(" |");
                                for (double v : nxt) sb.append(' ').append(h(v));
                            } catch (Exception e) {
                                sb.append(" | ERR");
                                break;
                            }
                            cur = nxt;
                        }
                    }
                    w.println(sb);
                }
            }
        }
    }
}
