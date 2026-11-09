#pragma once

// The math module's out-of-line functions are implemented in Rust (rust/revc/src/math.rs, checked
// bit for bit against the original C++ with tests/golden/math.txt). Layouts as in the headers.
class CVuVector;
class CQuaternion;

extern "C" {
void revc_vector_normalise(CVector *v);
void revc_vector_cross(CVector *out, const CVector *a, const CVector *b);
void revc_matrix_mul3x3(CVector *out, const CMatrix *m, const CVector *v);
void revc_matrix_mul3x3_transposed(CVector *out, const CVector *v, const CMatrix *m);
void revc_matrix_mul_point(CVector *out, const CMatrix *m, const CVector *v);
void revc_transform_points(CVuVector *out, int n, const CMatrix *m, const void *in, int stride);

void revc_quat_normalise(CQuaternion *q);
void revc_quat_slerp(CQuaternion *out, const CQuaternion *q1, const CQuaternion *q2, float theta, float invSin, float t);
void revc_quat_multiply(CQuaternion *out, const CQuaternion *q1, const CQuaternion *q2);
void revc_quat_get_axis_angle(const CQuaternion *q, RwV3d *axis, float *angle);
void revc_quat_set_axis_angle(CQuaternion *q, const RwV3d *axis, float angle);
void revc_quat_get_rw_matrix(const CQuaternion *q, RwMatrix *m);
void revc_quat_set_rw_matrix(CQuaternion *q, const RwMatrix *m);
void revc_quat_get_euler(const CQuaternion *q, float *f1, float *f2, float *f3);
void revc_quat_set_euler(CQuaternion *q, float f1, float f2, float f3);

void revc_matrix_set_unity(CMatrix *m);
void revc_matrix_reset_orientation(CMatrix *m);
void revc_matrix_set_scale(CMatrix *m, float s);
void revc_matrix_set_translate(CMatrix *m, float x, float y, float z);
void revc_matrix_set_rotate_x_only(CMatrix *m, float a);
void revc_matrix_set_rotate_y_only(CMatrix *m, float a);
void revc_matrix_set_rotate_z_only(CMatrix *m, float a);
void revc_matrix_set_rotate_x(CMatrix *m, float a);
void revc_matrix_set_rotate_y(CMatrix *m, float a);
void revc_matrix_set_rotate_z(CMatrix *m, float a);
void revc_matrix_set_rotate(CMatrix *m, float x, float y, float z);
void revc_matrix_rotate_x(CMatrix *m, float a);
void revc_matrix_rotate_y(CMatrix *m, float a);
void revc_matrix_rotate_z(CMatrix *m, float a);
void revc_matrix_rotate(CMatrix *m, float x, float y, float z);
void revc_matrix_reorthogonalise(CMatrix *m);
void revc_matrix_mul(CMatrix *out, const CMatrix *m1, const CMatrix *m2);
void revc_matrix_invert(const CMatrix *src, CMatrix *dst);
void revc_matrix_add_assign(CMatrix *m, const CMatrix *rhs);
void revc_matrix_assign(CMatrix *m, const CMatrix *rhs);
void revc_matrix_update(CMatrix *m);
void revc_matrix_update_rw(CMatrix *m);
void revc_matrix_copy_to_rw(const CMatrix *m, RwMatrix *rw);
void revc_matrix_compress(void *c, const CMatrix *m);
void revc_matrix_decompress(const void *c, CMatrix *m);

void revc_rect_init(CRect *r);
}
