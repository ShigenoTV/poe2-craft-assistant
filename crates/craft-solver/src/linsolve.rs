//! Systèmes linéaires creux `A x = b` de la forme `I − P` (P sous-stochastique : chaîne de Markov
//! d'une politique fixée). Sert à évaluer une politique d'un coup (policy iteration) et à compter les
//! visites espérées, là où l'itération simple converge en des centaines de milliers de passes quand
//! l'objectif est rare (la masse ne quitte la boucle « objet raté → base neuve » qu'au compte-gouttes).
//! Méthode : GMRES redémarré, préconditionné à droite par une factorisation LU incomplète ILU(0).

/// Matrice creuse par lignes, colonnes triées dans chaque ligne, diagonale présente.
pub struct Csr {
    pub n: usize,
    pub off: Vec<usize>,
    pub col: Vec<u32>,
    pub val: Vec<f64>,
}

impl Csr {
    /// Construit `I − P` depuis les lignes de P : `rows[i]` = (colonne, probabilité), sans la diagonale
    /// (boucle sur soi déjà résolue). Les doublons de colonne sont additionnés.
    pub fn identity_minus(n: usize, mut row: impl FnMut(usize, &mut Vec<(u32, f64)>)) -> Self {
        let (mut off, mut col, mut val) = (vec![0usize], Vec::new(), Vec::new());
        let mut buf: Vec<(u32, f64)> = Vec::new();
        for i in 0..n {
            buf.clear();
            row(i, &mut buf);
            buf.push((i as u32, -1.0)); // diagonale : 1 (signe inversé plus bas)
            buf.sort_unstable_by_key(|e| e.0);
            let start = col.len();
            for &(j, p) in &buf {
                if col.len() > start && *col.last().unwrap() == j {
                    *val.last_mut().unwrap() -= p;
                } else {
                    col.push(j);
                    val.push(-p);
                }
            }
            off.push(col.len());
        }
        Self { n, off, col, val }
    }

    pub fn transpose(&self) -> Self {
        let n = self.n;
        let mut cnt = vec![0usize; n + 1];
        for &j in &self.col {
            cnt[j as usize + 1] += 1;
        }
        for i in 0..n {
            cnt[i + 1] += cnt[i];
        }
        let off = cnt.clone();
        let mut pos = cnt;
        let mut col = vec![0u32; self.col.len()];
        let mut val = vec![0.0; self.val.len()];
        for i in 0..n {
            for k in self.off[i]..self.off[i + 1] {
                let j = self.col[k] as usize;
                col[pos[j]] = i as u32;
                val[pos[j]] = self.val[k];
                pos[j] += 1;
            }
        }
        Self { n, off, col, val }
    }

    fn mul(&self, x: &[f64], y: &mut [f64]) {
        for i in 0..self.n {
            let mut s = 0.0;
            for k in self.off[i]..self.off[i + 1] {
                s += self.val[k] * x[self.col[k] as usize];
            }
            y[i] = s;
        }
    }
}

/// Factorisation ILU(0) : L (diagonale unité) et U rangés dans la structure de A.
struct Ilu0 {
    a: Csr,
    diag: Vec<usize>,
}

impl Ilu0 {
    fn new(a: &Csr) -> Option<Self> {
        let n = a.n;
        let mut f = Csr { n, off: a.off.clone(), col: a.col.clone(), val: a.val.clone() };
        let mut diag = vec![usize::MAX; n];
        let mut iw = vec![usize::MAX; n];
        for i in 0..n {
            let (lo, hi) = (f.off[i], f.off[i + 1]);
            for k in lo..hi {
                iw[f.col[k] as usize] = k;
            }
            for k in lo..hi {
                let c = f.col[k] as usize;
                if c >= i {
                    break;
                }
                let piv = f.val[diag[c]];
                let lik = f.val[k] / piv;
                f.val[k] = lik;
                for kk in diag[c] + 1..f.off[c + 1] {
                    let w = iw[f.col[kk] as usize];
                    if w != usize::MAX {
                        f.val[w] -= lik * f.val[kk];
                    }
                }
            }
            for k in lo..hi {
                if f.col[k] as usize == i {
                    diag[i] = k;
                }
                iw[f.col[k] as usize] = usize::MAX;
            }
            if diag[i] == usize::MAX || f.val[diag[i]].abs() <= 1e-300 || !f.val[diag[i]].is_finite() {
                return None;
            }
        }
        Some(Self { a: f, diag })
    }

    /// x ← M⁻¹ x (descente L puis remontée U)
    fn apply(&self, x: &mut [f64]) {
        let f = &self.a;
        for i in 0..f.n {
            let mut s = x[i];
            for k in f.off[i]..self.diag[i] {
                s -= f.val[k] * x[f.col[k] as usize];
            }
            x[i] = s;
        }
        for i in (0..f.n).rev() {
            let mut s = x[i];
            for k in self.diag[i] + 1..f.off[i + 1] {
                s -= f.val[k] * x[f.col[k] as usize];
            }
            x[i] = s / f.val[self.diag[i]];
        }
    }
}

fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Résout `A x = b` (x sert de point de départ). `Some(résidu relatif)` si ‖b − A x‖ ≤ tol·‖b‖ est atteint,
/// ou si le résidu ne baisse plus (limite de la précision machine) en restant sous `accept`·‖b‖.
pub fn gmres(a: &Csr, b: &[f64], x: &mut [f64], tol: f64, accept: f64, max_iter: usize, mut stop: impl FnMut() -> bool) -> Option<f64> {
    let n = a.n;
    let bn = norm(b);
    if bn == 0.0 {
        x.iter_mut().for_each(|v| *v = 0.0);
        return Some(0.0);
    }
    let m = Ilu0::new(a)?;
    const RESTART: usize = 60;
    let mut r = vec![0.0; n];
    let mut w = vec![0.0; n];
    let mut z = vec![0.0; n];
    let mut basis: Vec<Vec<f64>> = (0..=RESTART).map(|_| vec![0.0; n]).collect();
    let mut h = vec![vec![0.0f64; RESTART]; RESTART + 1];
    let (mut cs, mut sn, mut g) = (vec![0.0f64; RESTART], vec![0.0f64; RESTART], vec![0.0f64; RESTART + 1]);
    let mut done = 0usize;
    let mut best = f64::INFINITY;
    let mut stalls = 0;
    loop {
        a.mul(x, &mut r);
        for i in 0..n {
            r[i] = b[i] - r[i];
        }
        let beta = norm(&r);
        let rel = beta / bn;
        if !rel.is_finite() {
            return None;
        }
        if rel <= tol {
            return Some(rel);
        }
        // un redémarrage qui ne gagne plus rien : précision machine atteinte
        if rel > best * 0.5 {
            stalls += 1;
            if stalls >= 3 {
                return (rel <= accept).then_some(rel);
            }
        } else {
            stalls = 0;
        }
        best = best.min(rel);
        if done >= max_iter || stop() {
            return None;
        }
        for i in 0..n {
            basis[0][i] = r[i] / beta;
        }
        g.iter_mut().for_each(|v| *v = 0.0);
        g[0] = beta;
        let mut k_used = 0;
        for k in 0..RESTART {
            z.copy_from_slice(&basis[k]);
            m.apply(&mut z);
            a.mul(&z, &mut w);
            for j in 0..=k {
                let hj: f64 = w.iter().zip(&basis[j]).map(|(p, q)| p * q).sum();
                h[j][k] = hj;
                for (wi, vi) in w.iter_mut().zip(&basis[j]) {
                    *wi -= hj * vi;
                }
            }
            let hn = norm(&w);
            h[k + 1][k] = hn;
            if hn > 0.0 {
                for i in 0..n {
                    basis[k + 1][i] = w[i] / hn;
                }
            }
            for j in 0..k {
                let t = cs[j] * h[j][k] + sn[j] * h[j + 1][k];
                h[j + 1][k] = -sn[j] * h[j][k] + cs[j] * h[j + 1][k];
                h[j][k] = t;
            }
            let d = (h[k][k] * h[k][k] + h[k + 1][k] * h[k + 1][k]).sqrt();
            if d == 0.0 {
                return None;
            }
            cs[k] = h[k][k] / d;
            sn[k] = h[k + 1][k] / d;
            h[k][k] = d;
            h[k + 1][k] = 0.0;
            g[k + 1] = -sn[k] * g[k];
            g[k] *= cs[k];
            k_used = k + 1;
            done += 1;
            if g[k + 1].abs() <= tol * 0.1 * bn || hn == 0.0 {
                break;
            }
        }
        // y = H⁻¹ g (triangulaire), x += M⁻¹ V y
        let mut y = vec![0.0f64; k_used];
        for i in (0..k_used).rev() {
            let mut s = g[i];
            for j in i + 1..k_used {
                s -= h[i][j] * y[j];
            }
            y[i] = s / h[i][i];
        }
        z.iter_mut().for_each(|v| *v = 0.0);
        for (j, yj) in y.iter().enumerate() {
            for (zi, vi) in z.iter_mut().zip(&basis[j]) {
                *zi += yj * vi;
            }
        }
        m.apply(&mut z);
        for i in 0..n {
            x[i] += z[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chaîne « tentative ratée → retour au départ » : x = visites espérées, connues en forme close.
    #[test]
    fn solves_a_rare_success_chain_exactly() {
        // états 0..k : chaque pas avance avec q, sinon retour en 0 ; succès = sortir de k−1
        let (k, q) = (5usize, 0.05f64);
        let a = Csr::identity_minus(k, |i, row| {
            if i + 1 < k {
                row.push((i as u32 + 1, q));
            }
            row.push((0, 1.0 - q));
        });
        // coût 1 par tentative : E[coût depuis 0] = Σ_{j<k} q^-(j+1) … vérifié par la récurrence
        let b = vec![1.0; k];
        let mut x = vec![0.0; k];
        gmres(&a, &b, &mut x, 1e-13, 1e-10, 500, || false).expect("convergé");
        // récurrence exacte : v_i = 1 + q v_{i+1} + (1−q) v_0, v_k = 0
        for i in 0..k {
            let next = if i + 1 < k { x[i + 1] } else { 0.0 };
            let rhs = 1.0 + q * next + (1.0 - q) * x[0];
            assert!((x[i] - rhs).abs() <= 1e-9 * x[0], "état {i} : {} vs {rhs}", x[i]);
        }
        assert!(x[0] > 1e6, "objectif très rare : coût énorme ({})", x[0]);
        // le transposé (visites) aussi
        let at = a.transpose();
        let mut e = vec![0.0; k];
        e[0] = 1.0;
        let mut vis = vec![0.0; k];
        gmres(&at, &e, &mut vis, 1e-13, 1e-10, 500, || false).expect("convergé");
        let total: f64 = vis.iter().sum();
        assert!((total - x[0]).abs() <= 1e-9 * x[0], "visites totales = coût à 1 par pas : {total} vs {}", x[0]);
    }
}
