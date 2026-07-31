#ifndef MEDIAPIPE_C_API_H
#define MEDIAPIPE_C_API_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct MP_Graph MP_Graph;
typedef struct MP_Session MP_Session;
typedef struct MP_Tensor MP_Tensor;

typedef enum {
    MP_OK = 0,
    MP_ERROR = 1,
    MP_INVALID_ARGUMENT = 2,
    MP_NOT_FOUND = 3,
    MP_OUT_OF_RANGE = 4,
} MP_Status;

typedef enum {
    MP_TENSOR_FLOAT32 = 0,
    MP_TENSOR_UINT8 = 1,
    MP_TENSOR_INT32 = 2,
    MP_TENSOR_INT64 = 3,
    MP_TENSOR_STRING = 4,
    MP_TENSOR_BOOL = 5,
    MP_TENSOR_INT16 = 6,
    MP_TENSOR_COMPLEX64 = 7,
    MP_TENSOR_INT8 = 8,
    MP_TENSOR_FLOAT16 = 9,
} MP_TensorType;

typedef struct {
    const char* name;
    int32_t* shape;
    int32_t shape_size;
    MP_TensorType type;
} MP_TensorInfo;

typedef struct {
    float x;
    float y;
    float z;
    float visibility;
    float presence;
} MP_Landmark;

typedef struct {
    MP_Landmark* landmarks;
    int32_t num_landmarks;
    int32_t handedness;
} MP_LandmarkResult;

typedef struct {
    float x_min;
    float y_min;
    float width;
    float height;
} MP_BoundingBox;

typedef struct {
    int32_t class_id;
    float score;
    const char* label;
    const char* display_name;
} MP_Category;

typedef struct {
    MP_BoundingBox bounding_box;
    MP_Category* categories;
    int32_t num_categories;
} MP_DetectionResult;

MP_Status mp_load_model(const char* model_path, MP_Graph** graph_out);

MP_Status mp_create_session(MP_Graph* graph, const char* calculator_graph_config, MP_Session** session_out);

void mp_delete_graph(MP_Graph* graph);

void mp_delete_session(MP_Session* session);

MP_Status mp_session_get_input_tensor_info(MP_Session* session, int32_t index, MP_TensorInfo* info_out);

MP_Status mp_session_get_output_tensor_info(MP_Session* session, int32_t index, MP_TensorInfo* info_out);

MP_Status mp_session_set_input_tensor(MP_Session* session, int32_t index, MP_TensorType type, const void* data, int32_t* shape, int32_t shape_size);

MP_Status mp_session_run(MP_Session* session);

MP_Status mp_session_get_output_tensor(MP_Session* session, int32_t index, MP_Tensor* tensor_out);

MP_TensorType mp_tensor_get_type(MP_Tensor* tensor);

void* mp_tensor_get_data(MP_Tensor* tensor);

int32_t* mp_tensor_get_shape(MP_Tensor* tensor, int32_t* shape_size_out);

size_t mp_tensor_get_byte_size(MP_Tensor* tensor);

void mp_tensor_free(MP_Tensor* tensor);

MP_Status mp_face_landmark_detect(MP_Session* session, const uint8_t* image_data, int32_t width, int32_t height, int32_t channels, MP_LandmarkResult** results_out, int32_t* num_results_out);

void mp_landmark_results_free(MP_LandmarkResult** results, int32_t num_results);

MP_Status mp_hand_landmark_detect(MP_Session* session, const uint8_t* image_data, int32_t width, int32_t height, int32_t channels, MP_LandmarkResult** results_out, int32_t* num_results_out);

MP_Status mp_object_detect(MP_Session* session, const uint8_t* image_data, int32_t width, int32_t height, int32_t channels, MP_DetectionResult** results_out, int32_t* num_results_out);

void mp_detection_results_free(MP_DetectionResult** results, int32_t num_results);

#ifdef __cplusplus
}
#endif

#endif
