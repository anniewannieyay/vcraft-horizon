import java.awt.*;
import java.awt.geom.*;
import java.awt.image.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.List;
import javax.imageio.ImageIO;

/** Draws the VCraft icon as separate layers and writes a layered PSD plus PNG previews. */
public class IconPsd {
    static final int S = 1024;

    record Layer(String name, BufferedImage img, int opacity) {}

    public static void main(String[] args) throws Exception {
        File out = new File(args[0]);
        out.mkdirs();
        Shape tile = new RoundRectangle2D.Double(0, 0, S, S, 460, 460);
        List<Layer> layers = new ArrayList<>();

        // 1. Background tile: green gradient.
        layers.add(new Layer("Background", draw(g -> {
            g.setPaint(new GradientPaint(0, 0, new Color(0x3BDC7E), S, S, new Color(0x08613A)));
            g.fill(tile);
        }), 255));

        // 2. Pixel grid: faint blocks, a nod to Minecraft without using its art.
        layers.add(new Layer("Pixel grid", draw(g -> {
            g.setClip(tile);
            int b = 64;
            for (int y = 0; y < S; y += b)
                for (int x = 0; x < S; x += b)
                    if (((x / b) + (y / b)) % 2 == 0) {
                        g.setColor(new Color(255, 255, 255, 16));
                        g.fillRect(x, y, b, b);
                    }
        }), 255));

        // 3. Back bubble (a friend's message), translucent.
        Shape back = bubble(372, 168, 500, 360, 72, false);
        layers.add(new Layer("Back bubble", draw(g -> {
            g.setColor(Color.WHITE);
            g.fill(back);
        }), 110));

        // 4. Shadow under the front bubble.
        Shape front = bubble(140, 330, 620, 440, 96, true);
        layers.add(new Layer("Bubble shadow", blur(draw(g -> {
            g.setColor(new Color(0, 40, 20, 120));
            g.translate(0, 26);
            g.fill(front);
        }), 22), 255));

        // 5. Front bubble.
        layers.add(new Layer("Front bubble", draw(g -> {
            g.setPaint(new GradientPaint(0, 330, new Color(0xFFFFFF), 0, 900, new Color(0xE3F8EA)));
            g.fill(front);
        }), 255));

        // 6. Voice bars inside the bubble.
        layers.add(new Layer("Voice bars", draw(g -> {
            int[] h = {120, 230, 320, 230, 120};
            int w = 60, gap = 36, cx = 450, cy = 550;
            int total = h.length * w + (h.length - 1) * gap;
            int x = cx - total / 2;
            g.setPaint(new GradientPaint(0, cy - 160, new Color(0x1DB863), 0, cy + 160, new Color(0x0B7A3F)));
            for (int hh : h) {
                g.fill(new RoundRectangle2D.Double(x, cy - hh / 2.0, w, hh, w, w));
                x += w + gap;
            }
        }), 255));

        // 7. Lock badge: end-to-end encryption.
        layers.add(new Layer("Lock badge", draw(g -> {
            int cx = 800, cy = 800, r = 158;
            g.setColor(new Color(0, 30, 15, 90));
            g.fill(new Ellipse2D.Double(cx - r, cy - r + 14, 2 * r, 2 * r));
            g.setColor(Color.WHITE);
            g.fill(new Ellipse2D.Double(cx - r, cy - r, 2 * r, 2 * r));
            int ri = r - 20;
            g.setPaint(new GradientPaint(0, cy - ri, new Color(0x173B5C), 0, cy + ri, new Color(0x0C1F33)));
            g.fill(new Ellipse2D.Double(cx - ri, cy - ri, 2 * ri, 2 * ri));
            // Shackle.
            g.setColor(Color.WHITE);
            g.setStroke(new BasicStroke(26, BasicStroke.CAP_BUTT, BasicStroke.JOIN_ROUND));
            Path2D sh = new Path2D.Double();
            sh.moveTo(cx - 44, cy + 2);
            sh.lineTo(cx - 44, cy - 34);
            sh.append(new Arc2D.Double(cx - 44, cy - 78, 88, 88, 180, -180, Arc2D.OPEN), true);
            sh.lineTo(cx + 44, cy + 2);
            g.draw(sh);
            // Body.
            g.fill(new RoundRectangle2D.Double(cx - 72, cy - 6, 144, 112, 26, 26));
            g.setColor(new Color(0x0C1F33));
            g.fill(new Ellipse2D.Double(cx - 14, cy + 28, 28, 28));
            g.fill(new Rectangle2D.Double(cx - 7, cy + 46, 14, 30));
        }), 255));

        // 8. Gloss on the top half of the tile.
        layers.add(new Layer("Gloss", draw(g -> {
            g.setClip(tile);
            g.setPaint(new GradientPaint(0, 0, new Color(255, 255, 255, 60), 0, S * 0.55f, new Color(255, 255, 255, 0)));
            g.fillRect(0, 0, S, S);
        }), 255));

        BufferedImage merged = new BufferedImage(S, S, BufferedImage.TYPE_INT_ARGB);
        Graphics2D mg = merged.createGraphics();
        for (Layer l : layers) {
            mg.setComposite(AlphaComposite.getInstance(AlphaComposite.SRC_OVER, l.opacity / 255f));
            mg.drawImage(l.img, 0, 0, null);
        }
        mg.dispose();

        writePsd(new File(out, "vcraft-icon.psd"), layers, merged);
        ImageIO.write(merged, "png", new File(out, "vcraft-icon-1024.png"));
        ImageIO.write(scale(merged, 512), "png", new File(out, "icon.png"));
        ImageIO.write(scale(merged, 64), "png", new File(out, "vcraft-icon-64-preview.png"));
        System.out.println("ok");
    }

    interface Painter { void paint(Graphics2D g); }

    static BufferedImage draw(Painter p) {
        BufferedImage img = new BufferedImage(S, S, BufferedImage.TYPE_INT_ARGB);
        Graphics2D g = img.createGraphics();
        g.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON);
        g.setRenderingHint(RenderingHints.KEY_STROKE_CONTROL, RenderingHints.VALUE_STROKE_PURE);
        g.setRenderingHint(RenderingHints.KEY_COLOR_RENDERING, RenderingHints.VALUE_COLOR_RENDER_QUALITY);
        p.paint(g);
        g.dispose();
        return img;
    }

    /** A rounded speech bubble with a short tail at the bottom (left or right). */
    static Shape bubble(double x, double y, double w, double h, double r, boolean tailLeft) {
        Area a = new Area(new RoundRectangle2D.Double(x, y, w, h, r * 2, r * 2));
        Path2D t = new Path2D.Double();
        if (tailLeft) {
            t.moveTo(x + 90, y + h - 40);
            t.lineTo(x + 40, y + h + 120);
            t.lineTo(x + 250, y + h - 10);
        } else {
            t.moveTo(x + w - 200, y + h - 10);
            t.lineTo(x + w - 20, y + h + 90);
            t.lineTo(x + w - 70, y + h - 40);
        }
        t.closePath();
        a.add(new Area(t));
        return a;
    }

    static BufferedImage blur(BufferedImage src, int radius) {
        BufferedImage img = src;
        for (int i = 0; i < 3; i++) img = boxBlur(boxBlur(img, radius, true), radius, false);
        return img;
    }

    static BufferedImage boxBlur(BufferedImage src, int r, boolean horiz) {
        int w = src.getWidth(), h = src.getHeight();
        int[] in = src.getRGB(0, 0, w, h, null, 0, w), out = new int[in.length];
        int n = 2 * r + 1;
        for (int o = 0; o < (horiz ? h : w); o++) {
            long sa = 0, sr = 0, sg = 0, sb = 0;
            int len = horiz ? w : h;
            for (int i = -r; i <= r; i++) {
                int p = in[idx(horiz, o, clamp(i, len), w)];
                int a = p >>> 24;
                sa += a; sr += (long) ((p >> 16) & 255) * a; sg += (long) ((p >> 8) & 255) * a; sb += (long) (p & 255) * a;
            }
            for (int i = 0; i < len; i++) {
                int a = (int) (sa / n);
                int rr = sa == 0 ? 0 : (int) (sr / sa), gg = sa == 0 ? 0 : (int) (sg / sa), bb = sa == 0 ? 0 : (int) (sb / sa);
                out[idx(horiz, o, i, w)] = (a << 24) | (rr << 16) | (gg << 8) | bb;
                int pOut = in[idx(horiz, o, clamp(i - r, len), w)], pIn = in[idx(horiz, o, clamp(i + r + 1, len), w)];
                int ao = pOut >>> 24, ai = pIn >>> 24;
                sa += ai - ao;
                sr += (long) ((pIn >> 16) & 255) * ai - (long) ((pOut >> 16) & 255) * ao;
                sg += (long) ((pIn >> 8) & 255) * ai - (long) ((pOut >> 8) & 255) * ao;
                sb += (long) (pIn & 255) * ai - (long) (pOut & 255) * ao;
            }
        }
        BufferedImage r2 = new BufferedImage(w, h, BufferedImage.TYPE_INT_ARGB);
        r2.setRGB(0, 0, w, h, out, 0, w);
        return r2;
    }

    static int clamp(int i, int len) { return Math.max(0, Math.min(len - 1, i)); }
    static int idx(boolean horiz, int o, int i, int w) { return horiz ? o * w + i : i * w + o; }

    static BufferedImage scale(BufferedImage src, int size) {
        BufferedImage cur = src;
        int s = src.getWidth();
        while (s / 2 >= size) {
            s /= 2;
            cur = resize(cur, s);
        }
        return s == size ? cur : resize(cur, size);
    }

    static BufferedImage resize(BufferedImage src, int s) {
        BufferedImage d = new BufferedImage(s, s, BufferedImage.TYPE_INT_ARGB);
        Graphics2D g = d.createGraphics();
        g.setRenderingHint(RenderingHints.KEY_INTERPOLATION, RenderingHints.VALUE_INTERPOLATION_BICUBIC);
        g.setRenderingHint(RenderingHints.KEY_RENDERING, RenderingHints.VALUE_RENDER_QUALITY);
        g.drawImage(src, 0, 0, s, s, null);
        g.dispose();
        return d;
    }

    // ---- PSD writer (version 1, RGB, 8 bits, RLE channels) ----

    static void writePsd(File f, List<Layer> layers, BufferedImage merged) throws IOException {
        ByteArrayOutputStream info = new ByteArrayOutputStream();
        DataOutputStream li = new DataOutputStream(info);
        li.writeShort(-layers.size()); // negative: merged image's first alpha channel is its transparency
        List<byte[][]> chans = new ArrayList<>();
        for (Layer l : layers) {
            byte[][] c = new byte[4][];
            int[] order = {-1, 0, 1, 2};
            for (int k = 0; k < 4; k++) c[k] = rleChannel(l.img, order[k]);
            chans.add(c);
            li.writeInt(0); li.writeInt(0); li.writeInt(S); li.writeInt(S);
            li.writeShort(4);
            for (int k = 0; k < 4; k++) { li.writeShort(order[k]); li.writeInt(2 + c[k].length); }
            li.writeBytes("8BIM"); li.writeBytes("norm");
            li.writeByte(l.opacity); li.writeByte(0); li.writeByte(0); li.writeByte(0);
            byte[] name = l.name.getBytes(StandardCharsets.US_ASCII);
            int nameLen = 1 + name.length, pad = (4 - nameLen % 4) % 4;
            byte[] luni = luni(l.name);
            li.writeInt(4 + 4 + nameLen + pad + luni.length);
            li.writeInt(0); // no layer mask
            li.writeInt(0); // no blending ranges
            li.writeByte(name.length); li.write(name); for (int i = 0; i < pad; i++) li.writeByte(0);
            li.write(luni);
        }
        for (byte[][] c : chans) for (byte[] ch : c) { li.writeShort(1); li.write(ch); }
        if (info.size() % 2 != 0) info.write(0);

        try (DataOutputStream o = new DataOutputStream(new BufferedOutputStream(new FileOutputStream(f)))) {
            o.writeBytes("8BPS"); o.writeShort(1); o.write(new byte[6]);
            o.writeShort(4); o.writeInt(S); o.writeInt(S); o.writeShort(8); o.writeShort(3);
            o.writeInt(0); // color mode data
            o.writeInt(0); // image resources
            o.writeInt(4 + info.size() + 4); // layer and mask info section
            o.writeInt(info.size());
            info.writeTo(o);
            o.writeInt(0); // global layer mask info
            // Merged image: RLE, all row counts first, then the rows; channel order R, G, B, A.
            o.writeShort(1);
            int[] mOrder = {0, 1, 2, -1};
            List<byte[]> rows = new ArrayList<>();
            for (int ch : mOrder) {
                int[] px = merged.getRGB(0, 0, S, S, null, 0, S);
                for (int y = 0; y < S; y++) rows.add(packBits(row(px, y, ch)));
            }
            for (byte[] r : rows) o.writeShort(r.length);
            for (byte[] r : rows) o.write(r);
        }
    }

    /** Unicode layer name block, so Photoshop shows the name as written. */
    static byte[] luni(String name) throws IOException {
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        DataOutputStream d = new DataOutputStream(b);
        d.writeBytes("8BIM"); d.writeBytes("luni");
        int len = 4 + name.length() * 2;
        int padded = len + (len % 4 == 0 ? 0 : 4 - len % 4);
        d.writeInt(padded);
        d.writeInt(name.length());
        for (char ch : name.toCharArray()) d.writeChar(ch);
        for (int i = len; i < padded; i++) d.writeByte(0);
        return b.toByteArray();
    }

    static byte[] rleChannel(BufferedImage img, int ch) throws IOException {
        int[] px = img.getRGB(0, 0, S, S, null, 0, S);
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        DataOutputStream d = new DataOutputStream(b);
        byte[][] rows = new byte[S][];
        for (int y = 0; y < S; y++) rows[y] = packBits(row(px, y, ch));
        for (byte[] r : rows) d.writeShort(r.length);
        for (byte[] r : rows) d.write(r);
        return b.toByteArray();
    }

    static byte[] row(int[] px, int y, int ch) {
        byte[] r = new byte[S];
        int shift = ch == -1 ? 24 : 16 - 8 * ch;
        for (int x = 0; x < S; x++) r[x] = (byte) (px[y * S + x] >>> shift);
        return r;
    }

    static byte[] packBits(byte[] d) {
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        int i = 0, n = d.length;
        while (i < n) {
            int run = 1;
            while (i + run < n && run < 128 && d[i + run] == d[i]) run++;
            if (run >= 2) {
                o.write(-(run - 1)); o.write(d[i]); i += run;
            } else {
                int start = i, lit = 0;
                while (i < n && lit < 128) {
                    if (i + 1 < n && d[i] == d[i + 1]) break;
                    i++; lit++;
                }
                o.write(lit - 1); o.write(d, start, lit);
            }
        }
        return o.toByteArray();
    }
}
