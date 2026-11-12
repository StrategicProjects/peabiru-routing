import java.io.*;
import java.nio.file.*;
import java.util.*;
import org.geotools.coverage.grid.GridCoverage2D;
import org.geotools.coverage.grid.io.AbstractGridFormat;
import org.geotools.coverage.grid.io.GridCoverage2DReader;
import org.geotools.coverage.grid.io.GridFormatFinder;
import org.geotools.geometry.DirectPosition2D;
import org.geotools.geometry.Envelope2D;
import org.geotools.referencing.CRS;
import org.geotools.referencing.crs.DefaultGeographicCRS;
import org.geotools.util.factory.Hints;
import org.opengis.referencing.crs.CoordinateReferenceSystem;
import org.opengis.referencing.operation.MathTransform;

/** Opens a raster as R5's RasterDataSourceSampler does.
 *  usage: Harness info <tif>
 *         Harness run <tif> <points hex lon,lat> <out>
 *  out lines: xbits ybits value(bits|OUT|ERR) */
public class Harness {
    public static void main(String[] a) throws Exception {
        File f = new File(a[1]);
        AbstractGridFormat format = GridFormatFinder.findFormat(f);
        Hints hints = new Hints(Hints.FORCE_LONGITUDE_FIRST_AXIS_ORDER, Boolean.TRUE);
        GridCoverage2DReader reader = format.getReader(f, hints);
        GridCoverage2D cov = reader.read(null);
        Envelope2D env = cov.getEnvelope2D();
        CoordinateReferenceSystem crs = cov.getCoordinateReferenceSystem2D();
        MathTransform t = CRS.findMathTransform(DefaultGeographicCRS.WGS84, crs);
        if (a[0].equals("info")) {
            System.out.println("format " + format.getClass().getName());
            System.out.println("CRS:\n" + crs);
            System.out.println("transform class " + t.getClass().getName());
            System.out.println("transform:\n" + t);
            System.out.println("envelope " + env);
            System.out.println("gridToCRS(center) " + cov.getGridGeometry().getGridToCRS2D());
            return;
        }
        if (a[0].equals("boundary")) {
            // usage: boundary <tif> <n> <out points>: WGS84 points whose grid coordinate is
            // within ~1e-9 pixel of a boundary between pixels (x, y or both), or of the envelope
            int n = Integer.parseInt(a[2]);
            MathTransform g2c = cov.getGridGeometry().getGridToCRS2D(); // pixel centres
            MathTransform c2w = t.inverse();
            java.util.SplittableRandom r = new java.util.SplittableRandom(3);
            int w = cov.getRenderedImage().getWidth(), h = cov.getRenderedImage().getHeight();
            try (PrintWriter o = new PrintWriter(new BufferedWriter(new FileWriter(a[3]), 1 << 20))) {
                for (int i = 0; i < n; i++) {
                    double gx = r.nextInt(w) + r.nextDouble();
                    double gy = r.nextInt(h) + r.nextDouble();
                    double eps = (r.nextDouble() * 2 - 1) * Math.pow(10, -6 - r.nextInt(6));
                    int k = i % 3;
                    if (k != 1) gx = Math.floor(gx) + 0.5 + eps;
                    if (k != 0) gy = Math.floor(gy) + 0.5 + eps * (r.nextBoolean() ? 1 : -1);
                    if (i % 10 == 0) gx = -0.5 + eps; // the envelope's west edge
                    if (i % 10 == 1) gx = w - 0.5 + eps; // and east edge
                    double[] p = {gx, gy}, c = new double[2], wgs = new double[2];
                    g2c.transform(p, 0, c, 0, 1);
                    c2w.transform(c, 0, wgs, 0, 1);
                    o.println(Long.toHexString(Double.doubleToRawLongBits(wgs[0])) + ","
                            + Long.toHexString(Double.doubleToRawLongBits(wgs[1])) + ","
                            + Long.toHexString(Double.doubleToRawLongBits(c[0])) + ","
                            + Long.toHexString(Double.doubleToRawLongBits(c[1])));
                }
            }
            return;
        }
        List<String> lines = Files.readAllLines(Paths.get(a[2]));
        try (PrintWriter w = new PrintWriter(new BufferedWriter(new FileWriter(a[3]), 1 << 20))) {
            double[] el = new double[1];
            for (String l : lines) {
                if (l.isEmpty()) continue;
                int c = l.indexOf(',');
                double lon = Double.longBitsToDouble(Long.parseUnsignedLong(l.substring(0, c), 16));
                double lat = Double.longBitsToDouble(Long.parseUnsignedLong(l.substring(c + 1), 16));
                DirectPosition2D in = new DirectPosition2D(lon, lat);
                DirectPosition2D out = new DirectPosition2D();
                String xs, ys, v;
                try {
                    t.transform(in, out);
                    xs = Long.toHexString(Double.doubleToRawLongBits(out.getOrdinate(0)));
                    ys = Long.toHexString(Double.doubleToRawLongBits(out.getOrdinate(1)));
                } catch (Exception e) {
                    w.println("TERR TERR TERR");
                    continue;
                }
                if (env.contains((org.opengis.geometry.DirectPosition) out)) {
                    try {
                        cov.evaluate((org.opengis.geometry.DirectPosition) out, el);
                        v = Long.toHexString(Double.doubleToRawLongBits(el[0]));
                    } catch (Exception e) {
                        v = "ERR";
                    }
                } else {
                    v = "OUT";
                }
                w.println(xs + " " + ys + " " + v);
            }
        }
    }
}
